use std::{collections::HashMap, fmt, sync::Arc, time::Instant};

use chrono::{DateTime, Duration, Utc};
use reqwest::{Client, Method, RequestBuilder, StatusCode, cookie::Jar, header::DATE};
use tokio::sync::mpsc;

use crate::{
    app::{BackendEvent, BatchSignUpReport, Course, SIGNUP_ATTEMPTS},
    backend::{
        clock::{BoundarySample, OffsetEstimate, OffsetSample, TimeProbe},
        scraper::{self, check_login_error, parse_courses},
    },
};

/// Default number of requests used by [`NetworkClient::measure_offset`]
pub const CLOCK_SAMPLES: usize = 8;

/// Pause between clock samples. Not a divisor of 1s, so samples land at
/// different sub-second phases of the server clock
const CLOCK_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

/// Number of second boundaries measured by [`NetworkClient::sync_offset`]
const BOUNDARY_ROUNDS: usize = 3;

/// Minimum spacing between two polls while waiting for a second boundary
const BOUNDARY_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

/// How early polling starts before the expected boundary, on top of the coarse uncertainty
const BOUNDARY_LEAD: Duration = Duration::milliseconds(150);

/// Give up on a boundary round after this long
const BOUNDARY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(2500);

/// Where the server time is read from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TimeSource {
    /// `Date` header of a `HEAD` request
    DateHead,
    /// `Date` header of a `GET` request
    DateGet,
    /// "Serverzeit" text on `mycorona.html`
    ServerzeitPage,
}

#[derive(Debug)]
pub enum NetworkError {
    /// The server rejected the session, e.g. after a login in the browser
    SessionExpired,
    /// Non-success status other than `401`/`403`
    Status(StatusCode),
    /// Request could not be sent or no response arrived
    Request(reqwest::Error),
    /// Response body could not be read
    Body(reqwest::Error),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SessionExpired => write!(f, "Session abgelaufen."),
            Self::Status(status) => write!(f, "Server-Fehler: Status {}", status),
            Self::Request(e) => write!(f, "Netzwerkfehler: {}", e),
            Self::Body(e) => write!(f, "Fehler beim Lesen der Antwort: {}", e),
        }
    }
}

impl std::error::Error for NetworkError {}

#[derive(Clone)]
pub struct NetworkClient {
    client: Client,
    cookie_jar: Arc<Jar>,
}

const BASE_URL: &str = "https://campusonline.uni-ulm.de/CoronaNG";

impl NetworkClient {
    pub fn new() -> Self {
        let cookie_jar = Arc::new(Jar::default());
        let client = Client::builder()
            .cookie_provider(Arc::clone(&cookie_jar))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:148.0) Gecko/20100101 Firefox/148.0")
            .build()
            .expect("Failed to create HTTP client");

        Self { client, cookie_jar }
    }

    /// Log in and report the outcome via `tx`. Returns `true` on success
    pub async fn login(&self, user: &str, pass: &str, tx: mpsc::Sender<BackendEvent>) -> bool {
        let login_url = format!("{}/index.html", BASE_URL);
        let mut body = HashMap::new();
        body.insert("uid", user);
        body.insert("password", pass);

        match self.client.post(login_url).form(&body).send().await {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(html) => match check_login_error(&html) {
                    Ok(user) => {
                        let _ = tx.send(BackendEvent::LoginSuccess(user)).await;
                        return true;
                    }
                    Err(msg) => {
                        let _ = tx.send(BackendEvent::LoginFailed(msg)).await;
                    }
                },
                Err(e) => {
                    let _ = tx
                        .send(BackendEvent::LoginFailed(format!(
                            "Failed to read response body: {}",
                            e
                        )))
                        .await;
                }
            },
            Ok(response) => {
                let err = format!("Server antwortete mit Status: {}", response.status());
                let _ = tx.send(BackendEvent::LoginFailed(err)).await;
            }
            Err(e) => {
                let _ = tx.send(BackendEvent::LoginFailed(e.to_string())).await;
            }
        }

        false
    }

    /// Coarse estimate of the server clock offset from `n` requests.
    /// Accurate to about half a second
    pub async fn measure_offset(&self, n: usize) -> Result<OffsetEstimate, String> {
        let source = self.detect_time_source().await?;
        self.coarse_offset(source, n).await
    }

    /// Estimate the server clock offset with sub-second precision.
    /// Falls back to the coarse estimate if no second boundary is detected
    pub async fn sync_offset(&self) -> Result<OffsetEstimate, String> {
        let source = self.detect_time_source().await?;
        let coarse = self.coarse_offset(source, CLOCK_SAMPLES).await?;

        let mut boundaries = Vec::with_capacity(BOUNDARY_ROUNDS);
        for _ in 0..BOUNDARY_ROUNDS {
            if let Ok(boundary) = self.find_second_boundary(source, &coarse).await {
                boundaries.push(boundary);
            }
        }

        Ok(OffsetEstimate::from_boundaries(&boundaries).unwrap_or(coarse))
    }

    /// Pick the first source that delivers the server time
    async fn detect_time_source(&self) -> Result<TimeSource, String> {
        let mut errors = Vec::new();
        for source in [
            TimeSource::DateHead,
            TimeSource::DateGet,
            TimeSource::ServerzeitPage,
        ] {
            match self.probe_time(source).await {
                Ok(_) => return Ok(source),
                Err(e) => errors.push(e),
            }
        }

        Err(format!(
            "Serverzeit konnte nicht bestimmt werden: {}",
            errors.join("; ")
        ))
    }

    async fn coarse_offset(&self, source: TimeSource, n: usize) -> Result<OffsetEstimate, String> {
        let mut samples = Vec::with_capacity(n);
        let mut last_err = None;

        for i in 0..n {
            if i > 0 {
                tokio::time::sleep(CLOCK_SAMPLE_INTERVAL).await;
            }

            match self.probe_time(source).await {
                Ok(probe) => samples.push(OffsetSample::from(probe)),
                Err(e) => last_err = Some(e),
            }
        }

        OffsetEstimate::from_samples(samples).ok_or_else(|| {
            format!(
                "Serverzeit konnte nicht bestimmt werden: {}",
                last_err.unwrap_or_else(|| "keine Messungen".to_string())
            )
        })
    }

    /// Poll shortly before the expected server second boundary until the server second changes
    async fn find_second_boundary(
        &self,
        source: TimeSource,
        coarse: &OffsetEstimate,
    ) -> Result<BoundarySample, String> {
        let lead = (coarse.uncertainty + BOUNDARY_LEAD).min(Duration::seconds(1));
        let server_now = Utc::now() + coarse.offset;
        let mut boundary = DateTime::from_timestamp(server_now.timestamp() + 1, 0)
            .ok_or("Ungültige Serverzeit")?;
        if boundary - coarse.offset - lead < Utc::now() {
            boundary += Duration::seconds(1);
        }

        let start = boundary - coarse.offset - lead;
        if let Ok(wait) = (start - Utc::now()).to_std() {
            tokio::time::sleep(wait).await;
        }

        let deadline = Instant::now() + BOUNDARY_TIMEOUT;
        let mut polled = Instant::now();
        let mut prev = self.probe_time(source).await?;

        while Instant::now() < deadline {
            tokio::time::sleep_until((polled + BOUNDARY_POLL_INTERVAL).into()).await;

            polled = Instant::now();
            let next = self.probe_time(source).await?;
            if let Some(sample) = BoundarySample::between(&prev, &next) {
                return Ok(sample);
            }
            prev = next;
        }

        Err("Sekundenwechsel der Serverzeit nicht erkannt".to_string())
    }

    async fn probe_time(&self, source: TimeSource) -> Result<TimeProbe, String> {
        let (method, url) = match source {
            TimeSource::DateHead => (Method::HEAD, format!("{}/index.html", BASE_URL)),
            TimeSource::DateGet => (Method::GET, format!("{}/index.html", BASE_URL)),
            TimeSource::ServerzeitPage => (Method::GET, format!("{}/user/mycorona.html", BASE_URL)),
        };

        let sent = Utc::now();
        let start = Instant::now();
        let response = self
            .client
            .request(method, url)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let server = if source == TimeSource::ServerzeitPage {
            let html = response.text().await.map_err(|e| e.to_string())?;
            scraper::get_server_time(&html)
                .ok_or("Serverzeit nicht in Seite gefunden")?
                .with_timezone(&Utc)
        } else {
            let date = response
                .headers()
                .get(DATE)
                .ok_or("Date-Header fehlt")?
                .to_str()
                .map_err(|e| e.to_string())?;
            DateTime::parse_from_rfc2822(date)
                .map_err(|e| format!("Ungültiger Date-Header '{}': {}", date, e))?
                .with_timezone(&Utc)
        };
        let rtt = Duration::from_std(start.elapsed()).map_err(|e| e.to_string())?;

        Ok(TimeProbe { sent, rtt, server })
    }

    /// Send `request` and return the response HTML of a logged-in page
    async fn fetch_page(&self, request: RequestBuilder) -> Result<String, NetworkError> {
        let response = request.send().await.map_err(NetworkError::Request)?;

        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(NetworkError::SessionExpired);
        }
        if !status.is_success() {
            return Err(NetworkError::Status(status));
        }

        let html = response.text().await.map_err(NetworkError::Body)?;
        // Expired sessions are redirected to the login form
        if html.contains("id=\"IndexForm\"") {
            return Err(NetworkError::SessionExpired);
        }

        Ok(html)
    }

    pub async fn get_courses(&self) -> Result<Vec<Course>, NetworkError> {
        let courses_url = format!("{}/user/mycorona.html", BASE_URL);
        let html = self.fetch_page(self.client.get(courses_url)).await?;
        Ok(parse_courses(&html))
    }

    /// Send one sign-up request for `ids` and return the response HTML
    async fn post_sign_up(&self, ids: &[String]) -> Result<String, NetworkError> {
        let courses_url = format!("{}/user/mycorona.html", BASE_URL);

        let mut body = HashMap::new();
        for id in ids {
            body.insert(format!("check_{}", id), "on");
            body.insert(format!("sort_{}", id), "000");
        }
        body.insert("action".into(), "5");
        body.insert("scope".into(), "inspections");

        let request = self
            .client
            .post(&courses_url)
            .header("Referer", &courses_url)
            .header("Origin", "https://campusonline.uni-ulm.de")
            .form(&body);

        self.fetch_page(request).await
    }

    pub async fn sign_up_courses(&self, ids: Vec<String>, tx: mpsc::Sender<BackendEvent>) {
        if ids.is_empty() {
            return;
        }

        let mut last_report: Option<BatchSignUpReport> = None;

        for i in 0..SIGNUP_ATTEMPTS {
            let attempt_num = (i + 1) as u32;

            let report = match self.post_sign_up(&ids).await {
                Ok(html) => {
                    let courses = parse_courses(&html);
                    let mut course_map = HashMap::new();
                    for c in &courses {
                        course_map.insert(c.name.clone(), c.id.clone());
                    }
                    let _ = tx.send(BackendEvent::CoursesUpdate(courses)).await;

                    scraper::parse_sign_up_results(&html, &course_map)
                }
                Err(e @ NetworkError::SessionExpired) => {
                    let _ = tx
                        .send(BackendEvent::SignUpAttempt(
                            attempt_num,
                            BatchSignUpReport::from_general_error(e.to_string()),
                        ))
                        .await;
                    let _ = tx
                        .send(BackendEvent::InternalMessage(
                            "Anmeldung wegen abgelaufener Session abgebrochen.".to_string(),
                        ))
                        .await;
                    return;
                }
                Err(e) => BatchSignUpReport::from_general_error(e.to_string()),
            };

            let _ = tx
                .send(BackendEvent::SignUpAttempt(attempt_num, report.clone()))
                .await;

            if report.total_success > 0 || report.total_failed > 0 {
                last_report = Some(report);
            }

            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }

        if let Some(report) = last_report {
            let _ = tx.send(BackendEvent::SignUpResult(report)).await;
        } else {
            let _ = tx
                .send(BackendEvent::InternalMessage(
                    "Anmeldung abgeschlossen, aber keine Ergebnisse extrahiert.".to_string(),
                ))
                .await;
        }
    }
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

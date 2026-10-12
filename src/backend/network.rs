use std::{collections::HashMap, sync::Arc, time::Instant};

use chrono::{DateTime, Duration, Utc};
use reqwest::{Client, Method, Response, cookie::Jar, header::DATE};
use tokio::sync::mpsc;

use crate::{
    app::{BackendEvent, BatchSignUpReport, SIGNUP_ATTEMPTS},
    backend::{
        clock::{OffsetEstimate, OffsetSample},
        scraper::{self, check_login_error, parse_courses},
    },
};

/// Default number of requests used by [`NetworkClient::measure_offset`]
pub const CLOCK_SAMPLES: usize = 8;

/// Pause between clock samples. Not a divisor of 1s, so samples land at
/// different sub-second phases of the server clock
const CLOCK_SAMPLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);

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

    pub async fn login(&self, user: &str, pass: &str, tx: mpsc::Sender<BackendEvent>) {
        let login_url = format!("{}/index.html", BASE_URL);
        let mut body = HashMap::new();
        body.insert("uid", user);
        body.insert("password", pass);

        match self.client.post(login_url).form(&body).send().await {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(html) => {
                    let result = check_login_error(&html);
                    if let Err(msg) = result {
                        let _ = tx.send(BackendEvent::LoginFailed(msg)).await;
                    } else {
                        let _ = tx
                            .send(BackendEvent::LoginSuccess(result.ok().unwrap()))
                            .await;
                    }
                }
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
    }

    /// Estimate the server clock offset from the `Date` header of `n` requests.
    /// Uses `HEAD` and switches to `GET` if `HEAD` fails
    pub async fn measure_offset(&self, n: usize) -> Result<OffsetEstimate, String> {
        let url = format!("{}/index.html", BASE_URL);
        let mut method = Method::HEAD;
        let mut samples = Vec::with_capacity(n);
        let mut last_err = None;

        for i in 0..n {
            if i > 0 {
                tokio::time::sleep(CLOCK_SAMPLE_INTERVAL).await;
            }

            let result = match self.sample_offset(&url, method.clone()).await {
                Err(_) if method == Method::HEAD => {
                    method = Method::GET;
                    self.sample_offset(&url, Method::GET).await
                }
                result => result,
            };

            match result {
                Ok(sample) => samples.push(sample),
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

    async fn sample_offset(&self, url: &str, method: Method) -> Result<OffsetSample, String> {
        let t0 = Utc::now();
        let start = Instant::now();
        let response = self
            .client
            .request(method, url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let rtt = Duration::from_std(start.elapsed()).map_err(|e| e.to_string())?;

        let date = response
            .headers()
            .get(DATE)
            .ok_or("Date-Header fehlt")?
            .to_str()
            .map_err(|e| e.to_string())?;
        let server_time = DateTime::parse_from_rfc2822(date)
            .map_err(|e| format!("Ungültiger Date-Header '{}': {}", date, e))?
            .with_timezone(&Utc);

        // The header truncates to whole seconds, so the true time is on average 500ms later
        let offset = server_time + Duration::milliseconds(500) - (t0 + rtt / 2);

        Ok(OffsetSample { offset, rtt })
    }

    pub async fn get_courses(&self, tx: mpsc::Sender<BackendEvent>) {
        let courses_url = format!("{}/user/mycorona.html", BASE_URL);

        match self.client.get(courses_url).send().await {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(html) => {
                    let courses = parse_courses(&html);
                    let _ = tx.send(BackendEvent::CoursesUpdate(courses)).await;
                }
                Err(e) => {
                    let err = format!("Failed to read HTML body: {}", e);
                    let _ = tx.send(BackendEvent::FetchFailed(err)).await;
                }
            },
            Ok(response) => {
                let err = format!("Server returned status: {}", response.status());
                let _ = tx.send(BackendEvent::FetchFailed(err)).await;
            }
            Err(e) => {
                let _ = tx.send(BackendEvent::FetchFailed(e.to_string())).await;
            }
        }
    }

    pub async fn sign_up_courses(&self, ids: Vec<String>, tx: mpsc::Sender<BackendEvent>) {
        if ids.is_empty() {
            return;
        }

        let courses_url = format!("{}/user/mycorona.html", BASE_URL);
        let mut last_report: Option<BatchSignUpReport> = None;

        for i in 0..SIGNUP_ATTEMPTS {
            let attempt_num = (i + 1) as u32;

            let mut body = HashMap::new();
            for id in &ids {
                body.insert(format!("check_{}", id), "on");
                body.insert(format!("sort_{}", id), "000");
            }
            body.insert("action".into(), "5");
            body.insert("scope".into(), "inspections");

            let result: Result<Response, reqwest::Error> = self
                .client
                .post(&courses_url)
                .header("Referer", &courses_url)
                .header("Origin", "https://campusonline.uni-ulm.de")
                .form(&body)
                .send()
                .await;

            let report = match result {
                Ok(response) if response.status().is_success() => match response.text().await {
                    Ok(html) => {
                        if html.contains("id=\"IndexForm\"") {
                            let _ = tx
                                .send(BackendEvent::SignUpAttempt(
                                    attempt_num,
                                    BatchSignUpReport::from_general_error(
                                        "Session abgelaufen.".to_string(),
                                    ),
                                ))
                                .await;
                            let _ = tx
                                .send(BackendEvent::InternalMessage(
                                    "Anmeldung wegen abgelaufener Session abgebrochen.".to_string(),
                                ))
                                .await;
                            return;
                        }

                        let courses = parse_courses(&html);
                        let mut course_map = HashMap::new();
                        for c in &courses {
                            course_map.insert(c.name.clone(), c.id.clone());
                        }
                        let _ = tx.send(BackendEvent::CoursesUpdate(courses)).await;

                        scraper::parse_sign_up_results(&html, &course_map)
                    }
                    Err(e) => BatchSignUpReport::from_general_error(format!(
                        "Fehler beim Lesen der Antwort: {}",
                        e
                    )),
                },
                Ok(response) => BatchSignUpReport::from_general_error(format!(
                    "Server-Fehler: Status {}",
                    response.status()
                )),
                Err(e) => BatchSignUpReport::from_general_error(format!("Netzwerkfehler: {}", e)),
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

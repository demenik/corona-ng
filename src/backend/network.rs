use std::sync::Arc;

use reqwest::{Client, cookie::Jar, multipart};
use tokio::sync::mpsc;

use crate::{
    app::BackendEvent,
    backend::scraper::{check_login_error, parse_courses},
};

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
        let form = multipart::Form::new()
            .text("uid", user.to_string())
            .text("password", pass.to_string());

        match self.client.post(login_url).multipart(form).send().await {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(html) => {
                    let err = check_login_error(&html);
                    if let Some(msg) = err {
                        let _ = tx.send(BackendEvent::LoginFailed(msg)).await;
                    } else {
                        let _ = tx.send(BackendEvent::LoginSuccess).await;
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
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

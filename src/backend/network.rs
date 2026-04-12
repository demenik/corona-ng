use std::sync::Arc;

use reqwest::{Client, cookie::Jar, multipart};
use tokio::sync::mpsc;

use crate::app::BackendEvent;

pub struct NetworkClient {
    client: Client,
    cookie_jar: Arc<Jar>,
}

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
        let login_url = "https://campusonline.uni-ulm.de/CoronaNG/index.html";
        let form = multipart::Form::new()
            .text("uid", user.to_string())
            .text("password", pass.to_string());

        match self.client.post(login_url).multipart(form).send().await {
            Ok(response) if response.status().is_success() => {
                let _ = tx.send(BackendEvent::LoginSuccess).await;
            }
            Ok(response) => {
                let err = format!("Server antwortete mit Status: {}", response.status());
                let _ = tx.send(BackendEvent::LoginFailed(err)).await;
            }
            Err(e) => {
                let _ = tx.send(BackendEvent::LoginFailed(e.to_string())).await;
            }
        }
    }
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

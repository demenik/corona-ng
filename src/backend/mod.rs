use std::{collections::HashMap, time::Duration};

use tokio::sync::mpsc;

use crate::{
    app::{BackendEvent, UiEvent},
    backend::network::NetworkClient,
};

pub mod network;

pub struct BackendState {
    pub schedule: HashMap<String, Vec<String>>,
    pub network: NetworkClient,
}

pub async fn run(mut ui_rx: mpsc::Receiver<UiEvent>, backend_tx: mpsc::Sender<BackendEvent>) {
    let mut state = BackendState {
        schedule: HashMap::new(),
        network: NetworkClient::new(),
    };

    let mut ticker = tokio::time::interval(Duration::from_millis(10));

    loop {
        tokio::select! {
            Some(ui_event) = ui_rx.recv() => {
                match ui_event {
                    UiEvent::Login(user, pass) => {
                        state.network.login(&user, &pass, backend_tx.clone()).await;
                    }
                    UiEvent::Quit => break,
                }
            }

            _ = ticker.tick() => {
                let now = chrono::Local::now();
                let time_str = now.format("%H:%M:%S%.3f").to_string();

                let _ = backend_tx.send(BackendEvent::ClockTick(time_str)).await;
            }
        }
    }
}

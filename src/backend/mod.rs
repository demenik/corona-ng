use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use tokio::sync::mpsc;

use crate::{
    app::{BackendEvent, UiEvent},
    backend::network::NetworkClient,
};

pub mod network;
pub mod scraper;

pub struct BackendState {
    pub schedule: HashMap<String, Vec<String>>,
    pub triggered_schedules: HashSet<(chrono::NaiveDate, String)>,
    pub network: NetworkClient,
}

pub async fn run(mut ui_rx: mpsc::Receiver<UiEvent>, backend_tx: mpsc::Sender<BackendEvent>) {
    let mut state = BackendState {
        schedule: HashMap::new(),
        triggered_schedules: HashSet::new(),
        network: NetworkClient::new(),
    };

    let mut ticker = tokio::time::interval(Duration::from_millis(10));
    let time_offset = Duration::from_millis(0); // Placeholder for local vs server offset

    loop {
        tokio::select! {
            Some(ui_event) = ui_rx.recv() => {
                match ui_event {
                    UiEvent::Login(user, pass) => {
                        state.network.login(&user, &pass, backend_tx.clone()).await;
                    }
                    UiEvent::Logout => {
                        state.network = NetworkClient::new();
                    }
                    UiEvent::FetchCourses => {
                        state.network.get_courses(backend_tx.clone()).await;
                    }
                    UiEvent::SetSchedule(course_id, time) => {
                        state.schedule.insert(course_id.clone(), vec![time.clone()]);
                        let _ = backend_tx.send(BackendEvent::InternalMessage(format!("Zeit für {} auf {} gesetzt.", course_id, time))).await;
                    }
                    UiEvent::DeleteSchedule(course_id) => {
                        state.schedule.remove(&course_id);
                        let _ = backend_tx.send(BackendEvent::InternalMessage(format!("Zeit für {} gelöscht.", course_id))).await;
                    }
                    UiEvent::Quit => break,
                }
            }

            _ = ticker.tick() => {
                let now = chrono::Local::now();
                let time_str = now.format("%H:%M:%S%.3f").to_string();
                let _ = backend_tx.send(BackendEvent::ClockTick(time_str)).await;

                // Collect courses for each schedule
                let mut schedules_to_trigger = HashMap::new();
                for (id, times) in &state.schedule {
                    for t in times {
                        schedules_to_trigger.entry(t.clone()).or_insert_with(Vec::new).push(id.clone());
                    }
                }

                let today = now.date_naive();

                for (target_time, ids) in schedules_to_trigger {
                    if state.triggered_schedules.contains(&(today, target_time.clone())) {
                        continue;
                    }

                    if let Ok(target) = chrono::NaiveTime::parse_from_str(&target_time, "%H:%M") {
                        let target_dt = today.and_time(target);
                        let target_dt_with_offset = target_dt - chrono::Duration::from_std(time_offset).unwrap();

                        let now_naive = now.naive_local();
                        let diff = target_dt_with_offset.signed_duration_since(now_naive);

                        // Trigger 200ms before target
                        if diff.num_milliseconds() <= 500 && diff.num_milliseconds() > -5000 {
                            state.triggered_schedules.insert((today, target_time.clone()));

                            let _ = backend_tx.send(BackendEvent::InternalMessage(format!("Anmeldung für {} gestartet!", target_time))).await;

                            let client = state.network.clone();
                            let btx = backend_tx.clone();
                            tokio::spawn(async move {
                                client.sign_up_courses(ids, btx).await;
                            });
                        }
                    }
                }

                // Cleanup triggered schedules from previous days
                state.triggered_schedules.retain(|(date, _)| *date >= today);
            }
        }
    }
}

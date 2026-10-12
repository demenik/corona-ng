use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    app::{BackendEvent, UiEvent},
    backend::{clock::ServerClock, network::NetworkClient},
    store::{Credentials, PersistentStore, SecureCredentialStore},
};

pub mod clock;
pub mod network;
pub mod scraper;

pub use clock::SERVER_TZ;

/// Interval between periodic clock syncs while logged in
const CLOCK_SYNC_INTERVAL: Duration = Duration::from_secs(600);

/// An extra clock sync starts when a scheduled target is at most this far away
const PRE_TARGET_SYNC_LEAD: chrono::Duration = chrono::Duration::seconds(60);

/// No clock sync starts when a scheduled target is this close, so sync requests
/// do not compete with the sign-up requests. A full sync takes up to about 12s
const SYNC_QUIET_WINDOW: chrono::Duration = chrono::Duration::seconds(30);

pub struct BackendState {
    pub schedule: HashMap<String, Vec<String>>,
    pub triggered_schedules: HashSet<(chrono::NaiveDate, String)>,
    pub network: NetworkClient,
    logged_in: bool,
    /// Start of the last clock sync attempt
    last_sync: Option<Instant>,
    sync_task: Option<JoinHandle<()>>,
    /// Targets that already received their pre-target clock sync
    pre_synced_schedules: HashSet<(chrono::NaiveDate, String)>,
}

impl BackendState {
    /// Start a clock sync in the background unless one is already running
    fn start_sync(&mut self, tx: &mpsc::Sender<BackendEvent>) {
        if self.sync_task.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }

        self.last_sync = Some(Instant::now());
        let client = self.network.clone();
        let tx = tx.clone();
        self.sync_task = Some(tokio::spawn(async move {
            sync_clock(&client, &tx).await;
        }));
    }

    pub async fn stored_credentials(&self) -> Option<Credentials> {
        if !self.logged_in {
            return None;
        }
        tokio::task::spawn_blocking(|| SecureCredentialStore::new().load())
            .await
            .ok()?
            .ok()
    }

    fn stop_sync(&mut self) {
        if let Some(task) = self.sync_task.take() {
            task.abort();
        }
        self.last_sync = None;
        self.pre_synced_schedules.clear();
    }
}

/// Measure the server clock offset and publish it to the shared clock
async fn sync_clock(client: &NetworkClient, tx: &mpsc::Sender<BackendEvent>) {
    match client.sync_offset().await {
        Ok(estimate) => {
            clock::set(ServerClock {
                offset: estimate.offset,
                synced_at: Some(Instant::now()),
                samples: estimate.samples,
            });
            let _ = tx
                .send(BackendEvent::ClockSynced {
                    offset_ms: estimate.offset.num_milliseconds(),
                    uncertainty_ms: estimate.uncertainty.num_milliseconds(),
                })
                .await;
        }
        Err(e) => {
            let _ = tx
                .send(BackendEvent::InternalMessage(format!(
                    "Zeitsynchronisation fehlgeschlagen: {}",
                    e
                )))
                .await;
        }
    }
}

pub async fn run(mut ui_rx: mpsc::Receiver<UiEvent>, backend_tx: mpsc::Sender<BackendEvent>) {
    let mut state = BackendState {
        schedule: HashMap::new(),
        triggered_schedules: HashSet::new(),
        network: NetworkClient::new(),
        logged_in: false,
        last_sync: None,
        sync_task: None,
        pre_synced_schedules: HashSet::new(),
    };

    let mut ticker = tokio::time::interval(Duration::from_millis(10));

    loop {
        tokio::select! {
            Some(ui_event) = ui_rx.recv() => {
                match ui_event {
                    UiEvent::Login(user, pass) => {
                        // The next tick starts the initial clock sync since `last_sync` is unset
                        state.logged_in = state.network.login(&user, &pass, backend_tx.clone()).await;
                    }
                    UiEvent::Logout => {
                        state.logged_in = false;
                        state.stop_sync();
                        state.network = NetworkClient::new();
                    }
                    UiEvent::FetchCourses => {
                        let event = match state.network.get_courses().await {
                            Ok(courses) => BackendEvent::CoursesUpdate(courses),
                            Err(e) => BackendEvent::FetchFailed(e.to_string()),
                        };
                        let _ = backend_tx.send(event).await;
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
                let now = clock::now();
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
                let mut target_imminent = false;

                for (target_time, ids) in schedules_to_trigger {
                    let Ok(target) = chrono::NaiveTime::parse_from_str(&target_time, "%H:%M") else {
                        continue;
                    };
                    let target_dt = today.and_time(target);
                    let diff = target_dt.signed_duration_since(now.naive_local());
                    let key = (today, target_time.clone());

                    if diff > -chrono::Duration::seconds(5) && diff <= SYNC_QUIET_WINDOW {
                        target_imminent = true;
                    }

                    if state.logged_in
                        && diff > SYNC_QUIET_WINDOW
                        && diff <= PRE_TARGET_SYNC_LEAD
                        && state.pre_synced_schedules.insert(key.clone())
                    {
                        state.start_sync(&backend_tx);
                    }

                    if state.triggered_schedules.contains(&key) {
                        continue;
                    }

                    // Trigger 200ms before target
                    if diff.num_milliseconds() <= 500 && diff.num_milliseconds() > -5000 {
                        state.triggered_schedules.insert(key);

                        let _ = backend_tx.send(BackendEvent::InternalMessage(format!("Anmeldung für {} gestartet!", target_time))).await;

                        let client = state.network.clone();
                        let btx = backend_tx.clone();
                        tokio::spawn(async move {
                            client.sign_up_courses(ids, btx).await;
                        });
                    }
                }

                if state.logged_in
                    && !target_imminent
                    && state.last_sync.is_none_or(|t| t.elapsed() >= CLOCK_SYNC_INTERVAL)
                {
                    state.start_sync(&backend_tx);
                }

                // Cleanup triggered schedules from previous days
                state.triggered_schedules.retain(|(date, _)| *date >= today);
                state.pre_synced_schedules.retain(|(date, _)| *date >= today);
            }
        }
    }
}

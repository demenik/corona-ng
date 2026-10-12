use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    app::{BackendEvent, UiEvent},
    backend::{
        clock::ServerClock,
        network::{NetworkClient, NetworkError},
    },
};

pub mod clock;
pub mod network;
pub mod scraper;

pub use clock::SERVER_TZ;

/// Interval between periodic clock syncs while logged in
const CLOCK_SYNC_INTERVAL: Duration = Duration::from_secs(600);

/// An extra clock sync and session check start when a scheduled target is at most this far away
const PRE_TARGET_SYNC_LEAD: chrono::Duration = chrono::Duration::seconds(60);

/// No clock sync starts when a scheduled target is this close, so sync requests
/// do not compete with the sign-up requests. A full sync takes up to about 12s
const SYNC_QUIET_WINDOW: chrono::Duration = chrono::Duration::seconds(30);

/// Interval between course fetches that keep the session alive while logged in
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(300);

pub struct BackendState {
    pub schedule: HashMap<String, Vec<String>>,
    pub triggered_schedules: HashSet<(chrono::NaiveDate, String)>,
    pub network: NetworkClient,
    /// Start of the last clock sync attempt
    last_sync: Option<Instant>,
    sync_task: Option<JoinHandle<()>>,
    /// Targets that already received their pre-target clock sync and session check
    prepared_schedules: HashSet<(chrono::NaiveDate, String)>,
    /// Last course fetch, manual or keep-alive
    last_keepalive: Option<Instant>,
    keepalive_task: Option<JoinHandle<()>>,
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

    /// Stop clock syncs and keep-alives after the session ended
    fn stop_background_tasks(&mut self) {
        if let Some(task) = self.sync_task.take() {
            task.abort();
        }
        if let Some(task) = self.keepalive_task.take() {
            task.abort();
        }
        self.last_sync = None;
        self.last_keepalive = None;
        self.prepared_schedules.clear();
    }

    /// Fetch courses in the background unless a keep-alive is already running.
    /// Refreshes the course list and renews an expired session early
    fn start_keepalive(&mut self, tx: &mpsc::Sender<BackendEvent>) {
        if self
            .keepalive_task
            .as_ref()
            .is_some_and(|t| !t.is_finished())
        {
            return;
        }

        self.last_keepalive = Some(Instant::now());
        let client = self.network.clone();
        let tx = tx.clone();
        self.keepalive_task = Some(tokio::spawn(async move {
            let event = match client.get_courses(&tx).await {
                Ok(courses) => BackendEvent::CoursesUpdate(courses),
                // A failed re-login already reported `LoginFailed`
                Err(NetworkError::SessionExpired) => return,
                Err(e) => BackendEvent::InternalMessage(format!(
                    "Kurse konnten nicht aktualisiert werden: {}",
                    e
                )),
            };
            let _ = tx.send(event).await;
        }));
    }

    /// Check the session in the background, so an expired session is renewed
    /// before the target instead of at it
    fn start_session_refresh(&self, tx: &mpsc::Sender<BackendEvent>) {
        let client = self.network.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            match client.refresh_session(&tx).await {
                // A failed re-login already reported `LoginFailed`
                Ok(()) | Err(NetworkError::SessionExpired) => {}
                Err(e) => {
                    let _ = tx
                        .send(BackendEvent::InternalMessage(format!(
                            "Sessionprüfung fehlgeschlagen: {}",
                            e
                        )))
                        .await;
                }
            }
        });
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
        last_sync: None,
        sync_task: None,
        prepared_schedules: HashSet::new(),
        last_keepalive: None,
        keepalive_task: None,
    };

    let mut ticker = tokio::time::interval(Duration::from_millis(10));

    loop {
        tokio::select! {
            Some(ui_event) = ui_rx.recv() => {
                match ui_event {
                    UiEvent::Login(user, pass) => {
                        // The next tick starts the initial clock sync since `last_sync` is unset
                        state.network.login(&user, &pass, backend_tx.clone()).await;
                        // The UI fetches courses right after `LoginSuccess`
                        state.last_keepalive = Some(Instant::now());
                    }
                    UiEvent::Logout => {
                        state.stop_background_tasks();
                        state.network.logout();
                    }
                    UiEvent::FetchCourses => {
                        state.last_keepalive = Some(Instant::now());
                        let event = match state.network.get_courses(&backend_tx).await {
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
                let logged_in = state.network.is_logged_in();
                // A failed re-login ends the session outside of `UiEvent::Logout`
                if !logged_in && (state.last_sync.is_some() || state.last_keepalive.is_some()) {
                    state.stop_background_tasks();
                }

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

                    if logged_in
                        && diff > SYNC_QUIET_WINDOW
                        && diff <= PRE_TARGET_SYNC_LEAD
                        && state.prepared_schedules.insert(key.clone())
                    {
                        state.start_session_refresh(&backend_tx);
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

                if logged_in
                    && !target_imminent
                    && state.last_sync.is_none_or(|t| t.elapsed() >= CLOCK_SYNC_INTERVAL)
                {
                    state.start_sync(&backend_tx);
                }

                if logged_in
                    && !target_imminent
                    && state.last_keepalive.is_none_or(|t| t.elapsed() >= KEEPALIVE_INTERVAL)
                {
                    state.start_keepalive(&backend_tx);
                }

                // Cleanup triggered schedules from previous days
                state.triggered_schedules.retain(|(date, _)| *date >= today);
                state.prepared_schedules.retain(|(date, _)| *date >= today);
            }
        }
    }
}

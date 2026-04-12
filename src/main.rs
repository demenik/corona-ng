mod app;
mod backend;
mod store;
mod ui;

use crate::app::{App, BackendEvent, CurrentScreen, SIGNUP_ATTEMPTS, SignUpOutcome, UiEvent};
use crate::store::{Credentials, JsonScheduleStore, PersistentStore, SecureCredentialStore};
use crate::ui::components::{
    Component, ComponentAction,
    signup_popup::{RequestDetail, RequestStatus, SignUpPopup},
};

use crossterm::event::KeyCode::{self, Char};
use ratatui::DefaultTerminal;
use std::{io, time::Duration};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> io::Result<()> {
    let (ui_tx, ui_rx) = mpsc::channel::<UiEvent>(32);
    let (backend_tx, mut backend_rx) = mpsc::channel::<BackendEvent>(32);

    let mut app = App::new(ui_tx);

    // Persistence
    let schedule_store = JsonScheduleStore::new();
    let credential_store = SecureCredentialStore::new();

    if let Ok(schedules) = schedule_store.load() {
        app.dashboard_screen.schedules = schedules.clone();
        // Inform backend about loaded schedules
        for (id, times) in schedules {
            for time in times {
                let _ = app
                    .tx
                    .try_send(UiEvent::SetSchedule(id.clone(), time.clone()));
            }
        }
    }

    if let Ok(creds) = credential_store.load() {
        app.login_screen
            .prefill(creds.username.clone(), creds.password.clone());
        app.last_credentials = Some(creds);
    }

    tokio::spawn(async move {
        backend::run(ui_rx, backend_tx).await;
    });

    let mut terminal = ratatui::init();

    let result = run_ui(
        &mut terminal,
        &mut app,
        &mut backend_rx,
        schedule_store,
        credential_store,
    );

    ratatui::restore();
    result
}

fn run_ui(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    backend_rx: &mut mpsc::Receiver<BackendEvent>,
    schedule_store: JsonScheduleStore,
    credential_store: SecureCredentialStore,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(10);
    let mut last_tick = std::time::Instant::now();

    loop {
        while let Ok(msg) = backend_rx.try_recv() {
            match msg {
                BackendEvent::ClockTick(time) => app.clock = time,
                BackendEvent::LoginSuccess => {
                    app.login_screen.is_loading = false;
                    let _ = app.tx.try_send(UiEvent::FetchCourses);
                    app.dashboard_screen.username = Some(app.login_screen.username.clone());
                    app.current_screen = CurrentScreen::Dashboard;

                    if let Some(creds) = &app.last_credentials {
                        match credential_store.save(creds) {
                            Ok(_) => app
                                .dashboard_screen
                                .set_status("Vorherige Anmeldedaten gespeichert.".to_string()),
                            Err(e) => app
                                .dashboard_screen
                                .set_status(format!("Konnte Login nicht speichern: {}", e)),
                        }
                    }
                }
                BackendEvent::LoginFailed(err) => {
                    app.login_screen.set_status(format!("Fehler: {}", err));
                }
                BackendEvent::CoursesUpdate(courses) => {
                    app.dashboard_screen.courses = Some(courses);
                }
                BackendEvent::FetchFailed(err) => {
                    app.dashboard_screen.set_status(format!("Fehler: {}", err));
                }
                BackendEvent::SignUpResult(report) => {
                    if let Some(popup) = &mut app.dashboard_screen.signup_popup {
                        popup.is_finished = true;

                        let last_idx = (SIGNUP_ATTEMPTS - 1) as u32;

                        // Populate errors from report
                        for detail in &report.details {
                            if let SignUpOutcome::Failed(err) = &detail.outcome {
                                if let Some(id) = &detail.course_id {
                                    let errors = popup.course_errors.entry(id.clone()).or_default();
                                    let entry = (err.clone(), last_idx);
                                    if !errors.contains(&entry) {
                                        errors.push(entry);
                                    }
                                } else if let Some(req_detail) =
                                    &mut popup.request_details[last_idx as usize]
                                    && !req_detail.errors.contains(err)
                                {
                                    req_detail.errors.push(err.clone());
                                }
                            }
                        }

                        // Mark all requests as either Success or Failed based on the final general_error status
                        for i in 0..SIGNUP_ATTEMPTS {
                            if popup.request_statuses[i] == RequestStatus::Waiting
                                || popup.request_statuses[i] == RequestStatus::Running
                            {
                                popup.request_statuses[i] = if report.general_error.is_none() {
                                    RequestStatus::Success
                                } else {
                                    RequestStatus::Failed
                                };

                                if let Some(err) = &report.general_error
                                    && let Some(detail) = &mut popup.request_details[i]
                                    && !detail.errors.contains(err)
                                {
                                    detail.errors.push(err.clone());
                                }
                            }
                        }
                    }
                    let _ = app.tx.try_send(UiEvent::FetchCourses);
                }
                BackendEvent::InternalMessage(msg) => {
                    app.dashboard_screen.set_status(msg);
                }
                BackendEvent::SignUpAttempt(attempt, report) => {
                    if let Some(popup) = &mut app.dashboard_screen.signup_popup {
                        let idx = (attempt as usize).saturating_sub(1);
                        if idx < SIGNUP_ATTEMPTS {
                            // Populate errors from report
                            for detail in &report.details {
                                if let SignUpOutcome::Failed(err) = &detail.outcome
                                    && let Some(id) = &detail.course_id
                                {
                                    let errors = popup.course_errors.entry(id.clone()).or_default();
                                    let entry = (err.clone(), idx as u32);
                                    if !errors.contains(&entry) {
                                        errors.push(entry);
                                    }
                                }
                            }

                            // Mark previous as Success if they were running and this one is starting
                            if idx > 0 && popup.request_statuses[idx - 1] == RequestStatus::Running
                            {
                                popup.request_statuses[idx - 1] = RequestStatus::Success;
                            }

                            // If this specific attempt failed with a general error, mark it Failed, otherwise Running
                            popup.request_statuses[idx] = if report.general_error.is_some() {
                                RequestStatus::Failed
                            } else {
                                RequestStatus::Running
                            };

                            // Update details with success/fail counts and local/server times
                            let local_time =
                                chrono::Local::now().format("%H:%M:%S%.3f").to_string();

                            let mut errors = Vec::new();
                            if let Some(err) = &report.general_error {
                                errors.push(err.clone());
                            }

                            // Also include course results that didn't match an ID as general errors for this request
                            for detail in &report.details {
                                if let SignUpOutcome::Failed(err) = &detail.outcome
                                    && detail.course_id.is_none()
                                    && !errors.contains(err)
                                {
                                    errors.push(err.clone());
                                }
                            }

                            popup.request_details[idx] = Some(RequestDetail {
                                success_count: report.total_success,
                                failed_count: report.total_failed,
                                local_time,
                                server_time: report.server_time.clone(),
                                errors,
                            });
                        }
                    }
                }
            }
        }

        // --- Auto-open Popup logic ---
        if app.current_screen == CurrentScreen::Dashboard {
            let mut target_to_open = None;
            let mut course_ids = Vec::new();
            let now = chrono::Local::now().time();

            // Find nearest schedule that is within 10 seconds
            for times in app.dashboard_screen.schedules.values() {
                for time_str in times {
                    if let Ok(target) = chrono::NaiveTime::parse_from_str(time_str, "%H:%M") {
                        let diff = target.signed_duration_since(now);
                        if diff.num_seconds() > 0 && diff.num_seconds() <= 10 {
                            target_to_open = Some(time_str.clone());
                            // Collect all course IDs for this target time
                            for (c_id, c_times) in &app.dashboard_screen.schedules {
                                if c_times.contains(time_str) {
                                    course_ids.push(c_id.clone());
                                }
                            }
                            break;
                        }
                    }
                }
                if target_to_open.is_some() {
                    break;
                }
            }

            if let Some(target) = target_to_open
                && app.dashboard_screen.signup_popup.is_none()
            {
                app.dashboard_screen.signup_popup = Some(SignUpPopup::new(target, course_ids));
            }
        }

        terminal.draw(|f| {
            ui::draw(f, app);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)?
            && let crossterm::event::Event::Key(key) = crossterm::event::read()?
        {
            if key.code == Char('c')
                && key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
            {
                let _ = app.tx.try_send(UiEvent::Quit);
                return Ok(());
            }

            let action = match app.current_screen {
                CurrentScreen::Start => app.start_screen.handle_key(key),
                CurrentScreen::Login => app.login_screen.handle_key(key),
                CurrentScreen::Dashboard => app.dashboard_screen.handle_key(key),
            };

            if let Some(act) = action {
                match act {
                    ComponentAction::ChangeScreen(new_screen) => app.current_screen = new_screen,
                    ComponentAction::TriggerLogin(user, pass) => {
                        app.last_credentials = Some(Credentials {
                            username: user.clone(),
                            password: pass.clone(),
                        });
                        let _ = app.tx.try_send(UiEvent::Login(user, pass));
                    }
                    ComponentAction::Logout => {
                        let _ = app.tx.try_send(UiEvent::Logout);
                        app.current_screen = CurrentScreen::Start;
                        app.login_screen.status_message = None;
                        app.login_screen.is_loading = false;
                        app.dashboard_screen.username = None;
                    }
                    ComponentAction::CoursesFetch => {
                        app.dashboard_screen
                            .set_status("Kurse werden aktualisiert...".to_string());
                        let _ = app.tx.try_send(UiEvent::FetchCourses);
                    }
                    ComponentAction::SetSchedule(course_id, time) => {
                        app.dashboard_screen
                            .schedules
                            .insert(course_id.clone(), vec![time.clone()]);
                        let _ = schedule_store.save(&app.dashboard_screen.schedules);
                        let _ = app.tx.try_send(UiEvent::SetSchedule(course_id, time));
                    }
                    ComponentAction::DeleteSchedule(course_id) => {
                        app.dashboard_screen.schedules.remove(&course_id);
                        let _ = schedule_store.save(&app.dashboard_screen.schedules);
                        let _ = app.tx.try_send(UiEvent::DeleteSchedule(course_id));
                    }
                    ComponentAction::Quit => return Ok(()),
                }
            }

            if key.code == KeyCode::Esc
                && let CurrentScreen::Login = app.current_screen
            {
                app.current_screen = CurrentScreen::Start;
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = std::time::Instant::now();
        }
    }
}

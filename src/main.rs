mod app;
mod backend;
mod ui;

use crate::app::{App, BackendEvent, CurrentScreen, UiEvent};
use crate::ui::components::{Component, ComponentAction};

use crossterm::event::KeyCode::{self, Char};
use ratatui::DefaultTerminal;
use std::{io, time::Duration};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> io::Result<()> {
    let (ui_tx, mut ui_rx) = mpsc::channel::<UiEvent>(32);
    let (backend_tx, mut backend_rx) = mpsc::channel::<BackendEvent>(32);

    let mut app = App::new(ui_tx);

    tokio::spawn(async move {
        backend::run(ui_rx, backend_tx).await;
    });

    let mut terminal = ratatui::init();

    let result = run_ui(&mut terminal, &mut app, &mut backend_rx);

    ratatui::restore();
    result
}

fn run_ui(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    backend_rx: &mut mpsc::Receiver<BackendEvent>,
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
                }
                BackendEvent::LoginFailed(err) => {
                    app.login_screen.set_status(format!("Fehler: {}", err));
                }
                BackendEvent::CoursesUpdate(courses) => {
                    app.dashboard_screen.courses = Some(courses);
                    app.dashboard_screen
                        .set_status("Kurse wurden aktualisiert.".to_string());
                }
                BackendEvent::FetchFailed(err) => {
                    app.dashboard_screen.set_status(format!("Fehler: {}", err));
                }
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
                    ComponentAction::Quit => return Ok(()),
                }
            }

            if key.code == KeyCode::Esc && !matches!(app.current_screen, CurrentScreen::Start) {
                app.current_screen = CurrentScreen::Start;
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = std::time::Instant::now();
        }
    }
}

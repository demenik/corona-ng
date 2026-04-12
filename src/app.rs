use crate::ui::components::start_screen::StartScreen;
use tokio::sync::mpsc;

pub enum CurrentScreen {
    Start,
    Login,
    Dashboard,
}

pub enum UiEvent {
    Login(String, String),
    Quit,
}

pub enum BackendEvent {
    ClockTick(String),
    LoginSuccess,
    LoginFailed(String),
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub start_screen: StartScreen,

    pub clock: String,
    pub is_logged_in: bool,
    pub login_error: Option<String>,

    pub tx: mpsc::Sender<UiEvent>,
}

impl App {
    pub fn new(tx: mpsc::Sender<UiEvent>) -> Self {
        Self {
            current_screen: CurrentScreen::Start,
            start_screen: StartScreen::new(),

            clock: "00:00:00.000".to_string(),
            is_logged_in: false,
            login_error: None,

            tx,
        }
    }
}

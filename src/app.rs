use crate::ui::components::{login_screen::LoginScreen, start_screen::StartScreen};
use tokio::sync::mpsc;

pub enum CurrentScreen {
    Start,
    Login,
    Dashboard,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CourseStatus {
    Open,
    Closed,
    Enrolled,
    Full,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct Course {
    pub id: String,
    pub name: String,
    pub note: String,
    pub observations: u32,
    pub participants: u32,
    pub max_participants: u32,
    pub status: CourseStatus,
}

pub enum UiEvent {
    Login(String, String),
    FetchCourses,
    Quit,
}

pub enum BackendEvent {
    ClockTick(String),
    LoginSuccess,
    LoginFailed(String),
    CoursesUpdate(Vec<Course>),
    FetchFailed(String),
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub start_screen: StartScreen,
    pub login_screen: LoginScreen,

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
            login_screen: LoginScreen::new(),

            clock: "00:00:00.000".to_string(),
            is_logged_in: false,
            login_error: None,

            tx,
        }
    }
}

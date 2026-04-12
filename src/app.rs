use crate::{
    store::Credentials,
    ui::components::{
        dashboard_screen::DashboardScreen, login_screen::LoginScreen, start_screen::StartScreen,
    },
};
use tokio::sync::mpsc;

pub const SIGNUP_ATTEMPTS: usize = 5;

#[derive(PartialEq)]
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

#[derive(Debug, Clone)]
pub enum SignUpOutcome {
    Success,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct CourseSignUpResult {
    pub course_id: Option<String>,
    pub course_name: String,
    pub outcome: SignUpOutcome,
}

#[derive(Debug, Clone)]
pub struct BatchSignUpReport {
    pub details: Vec<CourseSignUpResult>,
    pub total_success: u32,
    pub total_failed: u32,
    pub general_error: Option<String>,
    pub server_time: Option<String>,
}

impl BatchSignUpReport {
    pub fn from_general_error(error: String) -> Self {
        Self {
            details: Vec::new(),
            total_success: 0,
            total_failed: 0,
            general_error: Some(error),
            server_time: None,
        }
    }
}

pub enum UiEvent {
    Login(String, String),
    Logout,
    FetchCourses,
    SetSchedule(String, String),
    DeleteSchedule(String),
    Quit,
}

pub enum BackendEvent {
    ClockTick(String),
    LoginSuccess,
    LoginFailed(String),
    CoursesUpdate(Vec<Course>),
    FetchFailed(String),
    SignUpResult(BatchSignUpReport),
    SignUpAttempt(u32, BatchSignUpReport),
    InternalMessage(String),
}

pub struct App {
    pub current_screen: CurrentScreen,
    pub start_screen: StartScreen,
    pub login_screen: LoginScreen,
    pub dashboard_screen: DashboardScreen,

    pub clock: String,
    pub is_logged_in: bool,
    pub login_error: Option<String>,
    pub last_credentials: Option<Credentials>,

    pub tx: mpsc::Sender<UiEvent>,
}

impl App {
    pub fn new(tx: mpsc::Sender<UiEvent>) -> Self {
        Self {
            current_screen: CurrentScreen::Start,
            start_screen: StartScreen::new(),
            login_screen: LoginScreen::new(),
            dashboard_screen: DashboardScreen::new(),

            clock: "00:00:00.000".to_string(),
            is_logged_in: false,
            login_error: None,
            last_credentials: None,

            tx,
        }
    }
}

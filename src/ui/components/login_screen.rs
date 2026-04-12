use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{app::CurrentScreen, ui::components::Component};

use super::ComponentAction;

#[derive(PartialEq)]
enum LoginField {
    Username,
    Password,
    LoginBtn,
    CancelBtn,
}

pub struct LoginScreen {
    pub username: String,
    password: String,
    show_password: bool,
    focused_field: LoginField,
    pub is_loading: bool,
    pub status_message: Option<String>,
}

impl LoginScreen {
    pub fn new() -> Self {
        Self {
            username: String::new(),
            password: String::new(),
            show_password: false,
            focused_field: LoginField::Username,
            is_loading: false,
            status_message: None,
        }
    }

    pub fn set_status(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.is_loading = false;
    }

    pub fn prefill(&mut self, user: String, pass: String) {
        self.username = user;
        self.password = pass;
        if !self.username.is_empty() && !self.password.is_empty() {
            self.focused_field = LoginField::LoginBtn;
        }
    }
}

impl Default for LoginScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for LoginScreen {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ComponentAction> {
        if self.is_loading {
            return None;
        }

        match key.code {
            KeyCode::Down => {
                self.focused_field = match self.focused_field {
                    LoginField::Username => LoginField::Password,
                    LoginField::Password => LoginField::LoginBtn,
                    LoginField::LoginBtn => LoginField::Username,
                    LoginField::CancelBtn => LoginField::Username,
                };
                None
            }
            KeyCode::Up => {
                self.focused_field = match self.focused_field {
                    LoginField::Username => LoginField::LoginBtn,
                    LoginField::Password => LoginField::Username,
                    LoginField::LoginBtn => LoginField::Password,
                    LoginField::CancelBtn => LoginField::Password,
                };
                None
            }
            KeyCode::Left | KeyCode::Right => {
                self.focused_field = match self.focused_field {
                    LoginField::Username => LoginField::Username,
                    LoginField::Password => LoginField::Password,
                    LoginField::LoginBtn => LoginField::CancelBtn,
                    LoginField::CancelBtn => LoginField::LoginBtn,
                };
                None
            }
            KeyCode::Tab => {
                self.focused_field = match self.focused_field {
                    LoginField::Username => LoginField::Password,
                    LoginField::Password => LoginField::LoginBtn,
                    LoginField::LoginBtn => LoginField::CancelBtn,
                    LoginField::CancelBtn => LoginField::Username,
                };
                None
            }
            KeyCode::BackTab => {
                self.focused_field = match self.focused_field {
                    LoginField::Username => LoginField::CancelBtn,
                    LoginField::Password => LoginField::Username,
                    LoginField::LoginBtn => LoginField::Password,
                    LoginField::CancelBtn => LoginField::LoginBtn,
                };
                None
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_password = !self.show_password;
                None
            }
            KeyCode::Char(c) => {
                match self.focused_field {
                    LoginField::Username => self.username.push(c),
                    LoginField::Password => self.password.push(c),
                    _ => {}
                }
                None
            }
            KeyCode::Backspace => {
                match self.focused_field {
                    LoginField::Username => {
                        self.username.pop();
                    }
                    LoginField::Password => {
                        self.password.pop();
                    }
                    _ => {}
                }
                None
            }
            KeyCode::Enter => match self.focused_field {
                LoginField::LoginBtn => {
                    self.is_loading = true;
                    self.status_message = Some("Einloggen...".to_string());
                    Some(ComponentAction::TriggerLogin(
                        self.username.clone(),
                        self.password.clone(),
                    ))
                }
                LoginField::CancelBtn => Some(ComponentAction::ChangeScreen(CurrentScreen::Start)),
                _ => None,
            },
            _ => None,
        }
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(15),
                Constraint::Min(0),
            ])
            .split(area)[1];

        let area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(50),
                Constraint::Min(0),
            ])
            .split(area)[1];

        f.render_widget(Clear, area);
        let block = Block::default().borders(Borders::ALL).title(" Login ");
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3), // Username
                Constraint::Length(3), // Password
                Constraint::Length(3), // Status/Loading
                Constraint::Length(3), // Buttons
            ])
            .split(area);

        // --- Username ---
        let user_style = if self.focused_field == LoginField::Username {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new(self.username.as_str()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Nutzername ")
                    .border_style(user_style),
            ),
            chunks[0],
        );

        // --- Password ---
        let pass_display = if self.show_password {
            self.password.clone()
        } else {
            self.password.chars().map(|_| '*').collect()
        };
        let pass_style = if self.focused_field == LoginField::Password {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new(pass_display).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Passwort (Strg+S zum zeigen) ")
                    .border_style(pass_style),
            ),
            chunks[1],
        );

        // --- Status / Loading ---
        if let Some(ref msg) = self.status_message {
            let color = if self.is_loading {
                Color::Cyan
            } else {
                Color::Red
            };
            f.render_widget(
                Paragraph::new(msg.as_str()).style(Style::default().fg(color)),
                chunks[2],
            );
        }

        // --- Buttons ---
        let btn_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[3]);

        let login_btn_style = if self.focused_field == LoginField::LoginBtn {
            Style::default().bg(Color::Blue).fg(Color::Black)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new("Login")
                .alignment(ratatui::layout::Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(login_btn_style),
                ),
            btn_chunks[0],
        );

        let cancel_btn_style = if self.focused_field == LoginField::CancelBtn {
            Style::default().bg(Color::Red).fg(Color::Black)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new("Abbrechen")
                .alignment(ratatui::layout::Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(cancel_btn_style),
                ),
            btn_chunks[1],
        );
    }
}

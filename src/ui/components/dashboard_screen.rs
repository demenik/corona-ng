use chrono::{Local, NaiveTime};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};
use std::collections::HashMap;

use crate::{
    app::{Course, CourseStatus},
    ui::components::{Component, time_input::TimeInput},
};

use super::ComponentAction;

#[derive(PartialEq)]
enum DashboardFocus {
    CourseList,
    CountdownList,
    ReloadBtn,
    LogoutBtn,
}

pub struct DashboardScreen {
    pub courses: Option<Vec<Course>>,
    pub schedules: HashMap<String, Vec<String>>,
    focus: DashboardFocus,
    selected_course_idx: usize,

    pub status_message: Option<String>,
    pub username: Option<String>,
    pub time_input: Option<TimeInput>,
}

impl DashboardScreen {
    pub fn new() -> Self {
        Self {
            courses: None,
            schedules: HashMap::new(),
            focus: DashboardFocus::CourseList,
            selected_course_idx: 0,

            status_message: None,
            username: None,
            time_input: None,
        }
    }

    pub fn set_status(&mut self, status: String) {
        self.status_message = Some(status);
    }

    fn calculate_countdown(&self, target_time_str: &str) -> String {
        let now = Local::now().time();
        if let Ok(target) = NaiveTime::parse_from_str(target_time_str, "%H:%M") {
            let diff = target.signed_duration_since(now);
            if diff.num_seconds() <= 0 {
                return "00:00:00.000".to_string();
            }
            format!(
                "{:02}:{:02}:{:02}.{:03}",
                diff.num_hours(),
                diff.num_minutes() % 60,
                diff.num_seconds() % 60,
                diff.num_milliseconds() % 1000
            )
        } else {
            "--:--:--".into()
        }
    }
}

impl Default for DashboardScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for DashboardScreen {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ComponentAction> {
        if let Some(time_input) = &mut self.time_input {
            if key.code == KeyCode::Esc {
                self.time_input = None;
                return None;
            }
            let action = time_input.handle_key(key);
            if matches!(action, Some(ComponentAction::SetSchedule(_, _))) {
                self.time_input = None;
            }
            return action;
        }

        let course_count = self.courses.as_ref().map_or(0, |c| c.len());

        match key.code {
            KeyCode::Right => {
                self.focus = match self.focus {
                    DashboardFocus::CourseList => DashboardFocus::CountdownList,
                    DashboardFocus::CountdownList => DashboardFocus::CountdownList,
                    DashboardFocus::ReloadBtn => DashboardFocus::ReloadBtn,
                    DashboardFocus::LogoutBtn => DashboardFocus::LogoutBtn,
                };
            }
            KeyCode::Left => {
                self.focus = match self.focus {
                    DashboardFocus::CourseList => DashboardFocus::CourseList,
                    DashboardFocus::CountdownList => DashboardFocus::CourseList,
                    DashboardFocus::ReloadBtn => DashboardFocus::CourseList,
                    DashboardFocus::LogoutBtn => DashboardFocus::CourseList,
                };
            }
            KeyCode::Up => match self.focus {
                DashboardFocus::CourseList if self.selected_course_idx > 0 => {
                    self.selected_course_idx -= 1;
                }
                DashboardFocus::ReloadBtn => self.focus = DashboardFocus::CountdownList,
                DashboardFocus::LogoutBtn => self.focus = DashboardFocus::ReloadBtn,
                _ => (),
            },
            KeyCode::Down => match self.focus {
                DashboardFocus::CourseList
                    if course_count > 0 && self.selected_course_idx < course_count - 1 =>
                {
                    self.selected_course_idx += 1;
                }
                DashboardFocus::CountdownList => self.focus = DashboardFocus::ReloadBtn,
                DashboardFocus::ReloadBtn => self.focus = DashboardFocus::LogoutBtn,
                _ => (),
            },
            KeyCode::Enter => match self.focus {
                DashboardFocus::ReloadBtn => return Some(ComponentAction::CoursesFetch),
                DashboardFocus::LogoutBtn => return Some(ComponentAction::Logout),
                DashboardFocus::CourseList => {
                    if let Some(courses) = &self.courses
                        && let Some(course) = courses.get(self.selected_course_idx)
                    {
                        let last_time = self
                            .schedules
                            .get(&course.id)
                            .and_then(|v| v.last())
                            .map(|s| s.as_str());
                        self.time_input = Some(TimeInput::new(course.id.clone(), last_time));
                    }
                }

                _ => {}
            },
            _ => {}
        }
        None
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(0),    // Body
            ])
            .split(area);

        let body_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),    // Courses
                Constraint::Length(25), // Sidebar
            ])
            .split(main_layout[1]);

        // --- HEADER ---
        let header = Paragraph::new(format!(
            "Hallo {}. Du hast {} geladene Kurse\n{}",
            self.username.as_ref().unwrap_or(&"Nutzer".to_string()),
            self.courses.as_ref().map_or(0, |c| c.len()),
            self.status_message.clone().unwrap_or("".to_string())
        ))
        .block(Block::default().borders(Borders::BOTTOM))
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().add_modifier(Modifier::BOLD));
        f.render_widget(header, main_layout[0]);

        // --- COURSES ---
        let course_border_style = if self.focus == DashboardFocus::CourseList {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };
        let course_block = Block::default()
            .borders(Borders::ALL)
            .title(" Kurse (↕ Navigieren, Enter für Popup) ")
            .border_style(course_border_style);

        let rows: Vec<Row> = if let Some(courses) = &self.courses {
            courses
                .iter()
                .enumerate()
                .map(|(i, course)| {
                    let is_selected =
                        self.focus == DashboardFocus::CourseList && i == self.selected_course_idx;

                    let has_schedule = self.schedules.get(&course.id).and_then(|v| v.last());

                    let btn_text = match has_schedule {
                        Some(time) => format!("[ {} ]", time),
                        None => "[ Zeit setzen ]".to_string(),
                    };

                    let btn_style = if is_selected {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Magenta)
                            .add_modifier(Modifier::BOLD)
                    } else if has_schedule.is_some() {
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let row_style = if is_selected {
                        Style::default().bg(Color::DarkGray)
                    } else {
                        Style::default()
                    };

                    let status_str = match course.status {
                        CourseStatus::Open => "Offen",
                        CourseStatus::Closed => "Geschlossen",
                        CourseStatus::Enrolled => "Beigetreten",
                        CourseStatus::Full => "Voll",
                        CourseStatus::Unknown => "Unbekannt",
                    };

                    Row::new(vec![
                        Cell::from(course.name.clone()),
                        Cell::from(status_str),
                        Cell::from(
                            Text::from(course.observations.to_string())
                                .alignment(HorizontalAlignment::Right),
                        ),
                        Cell::from(
                            Text::from(format!(
                                "{}/{}",
                                course.participants, course.max_participants
                            ))
                            .alignment(HorizontalAlignment::Right),
                        ),
                        Cell::from(ratatui::text::Span::styled(btn_text, btn_style)),
                    ])
                    .style(row_style)
                })
                .collect()
        } else {
            vec![Row::new(vec!["Lade Kurse...", "", ""])]
        };

        let table = Table::new(
            rows,
            [
                Constraint::Fill(1),
                Constraint::Length(11),
                Constraint::Length(10),
                Constraint::Length(11),
                Constraint::Length(15),
            ],
        )
        .block(course_block)
        .header(
            Row::new(vec![
                "Name",
                "Status",
                "Beobachter",
                "Teilnehmer",
                "Anmeldezeit",
            ])
            .style(Style::default().add_modifier(Modifier::UNDERLINED)),
        );

        f.render_widget(table, body_layout[0]);

        // --- SIDEBAR ---
        let sidebar_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Live Clock
                Constraint::Min(0),    // Countdowns
                Constraint::Length(3), // Reload Button
                Constraint::Length(3), // Logout Button
            ])
            .split(body_layout[1]);

        // Live Clock
        let now = Local::now().format("%H:%M:%S%.3f").to_string();
        f.render_widget(
            Paragraph::new(now)
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL).title(" Live Zeit ")),
            sidebar_chunks[0],
        );

        // Countdowns
        let countdown_border = if self.focus == DashboardFocus::CountdownList {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };

        let target_times = ["16:00", "16:30"];

        let countdown_lines: Vec<String> = target_times
            .iter()
            .map(|&t| format!("{} -> {}", t, self.calculate_countdown(t)))
            .collect();

        let countdown_text = countdown_lines.join("\n");
        let countdown_widget = Paragraph::new(countdown_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Countdowns ")
                    .border_style(countdown_border),
            )
            .alignment(Alignment::Center);
        f.render_widget(countdown_widget, sidebar_chunks[1]);

        // Reload Button
        let reload_style = if self.focus == DashboardFocus::ReloadBtn {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        f.render_widget(
            Paragraph::new("Kurse neuladen")
                .alignment(ratatui::layout::Alignment::Center)
                .block(Block::default().borders(Borders::ALL).style(reload_style)),
            sidebar_chunks[2],
        );

        // Logout Button
        let logout_style = if self.focus == DashboardFocus::LogoutBtn {
            Style::default()
                .bg(Color::Red)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        f.render_widget(
            Paragraph::new("Ausloggen")
                .alignment(ratatui::layout::Alignment::Center)
                .block(Block::default().borders(Borders::ALL).style(logout_style)),
            sidebar_chunks[3],
        );

        if let Some(time_input) = &self.time_input {
            time_input.draw(f, area);
        }
    }
}

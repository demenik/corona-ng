use crate::app::{Course, CourseStatus, SIGNUP_ATTEMPTS};
use crate::ui::components::spinner::Spinner;
use chrono::{Local, NaiveTime};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table},
};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
pub enum RequestStatus {
    Waiting,
    Running,
    Success,
    Failed,
}

pub struct RequestDetail {
    pub success_count: u32,
    pub failed_count: u32,
    pub local_time: String,
    pub server_time: Option<String>,
    pub errors: Vec<String>,
}

pub struct SignUpPopup {
    pub target_time: String,
    pub request_statuses: [RequestStatus; SIGNUP_ATTEMPTS],
    pub request_details: [Option<RequestDetail>; SIGNUP_ATTEMPTS],
    pub course_ids: Vec<String>,
    pub course_errors: HashMap<String, Vec<(String, u32)>>, // (error, attempt_idx)
    pub is_finished: bool,
    pub spinner: Spinner,
}

impl SignUpPopup {
    pub fn new(target_time: String, course_ids: Vec<String>) -> Self {
        Self {
            target_time,
            request_statuses: [RequestStatus::Waiting; SIGNUP_ATTEMPTS],
            request_details: [const { None }; SIGNUP_ATTEMPTS],
            course_ids,
            course_errors: HashMap::new(),
            is_finished: false,
            spinner: Spinner::new(),
        }
    }

    fn calculate_countdown(&self) -> String {
        let now = Local::now().time();
        if let Ok(target) = NaiveTime::parse_from_str(&self.target_time, "%H:%M") {
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

    pub fn draw(&self, f: &mut Frame, area: Rect, courses: &[Course]) {
        let popup_area = centered_rect(70, 70, area);
        f.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Automatisches Anmelden für {} ", self.target_time))
            .border_style(Style::default().fg(Color::Cyan));
        f.render_widget(block, popup_area);

        let inner_area = popup_area.inner(Margin {
            vertical: 1,
            horizontal: 1,
        });

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Countdown
                Constraint::Fill(2),   // Requests + Errors
                Constraint::Fill(3),   // Courses
            ])
            .split(inner_area);

        // 1. Countdown
        let countdown = self.calculate_countdown();
        f.render_widget(
            Paragraph::new(format!("Zeit bis Start: {}", countdown))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::BOTTOM))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            chunks[0],
        );

        // 2. Requests Table
        let mut rows = Vec::new();
        for i in 0..SIGNUP_ATTEMPTS {
            let status = self.request_statuses[i];
            let (icon, style) = match status {
                RequestStatus::Waiting => ("🕒", Style::default().fg(Color::DarkGray)),
                RequestStatus::Running => {
                    (self.spinner.frame(), Style::default().fg(Color::Yellow))
                }
                RequestStatus::Success => ("✅", Style::default().fg(Color::Green)),
                RequestStatus::Failed => ("❌", Style::default().fg(Color::Red)),
            };

            let (s_text, f_text, local_text, server_text, errors) =
                if let Some(detail) = &self.request_details[i] {
                    (
                        detail.success_count.to_string(),
                        detail.failed_count.to_string(),
                        detail.local_time.clone(),
                        detail.server_time.clone().unwrap_or_default(),
                        &detail.errors,
                    )
                } else {
                    (
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        "".to_string(),
                        &vec![],
                    )
                };

            rows.push(
                Row::new(vec![
                    Cell::from(icon),
                    Cell::from(format!("Anfrage #{}", i + 1)),
                    Cell::from(s_text).style(Style::default().fg(Color::Green)),
                    Cell::from(f_text).style(Style::default().fg(Color::Red)),
                    Cell::from(local_text),
                    Cell::from(server_text),
                ])
                .style(style),
            );

            // General errors for this request
            for err in errors {
                rows.push(Row::new(vec![
                    Cell::from(""),
                    Cell::from(Line::from(vec![
                        Span::styled("  └─ ⚠️ ", Style::default().fg(Color::Yellow)),
                        Span::styled(err.clone(), Style::default().fg(Color::Red)),
                    ])),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                    Cell::from(""),
                ]));
            }
        }

        let table = Table::new(
            rows,
            [
                Constraint::Length(2),
                Constraint::Fill(1),
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Length(15),
                Constraint::Length(11),
            ],
        )
        .header(
            Row::new(vec![
                Cell::from(""),
                Cell::from("Anfrage"),
                Cell::from("✅"),
                Cell::from("❌"),
                Cell::from("Local Time"),
                Cell::from("Server Time"),
            ])
            .style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        )
        .block(
            Block::default()
                .title(" Status Anfragen ")
                .borders(Borders::BOTTOM),
        );
        f.render_widget(table, chunks[1]);

        // 3. Courses
        let courses_to_show: Vec<&Course> = courses
            .iter()
            .filter(|c| self.course_ids.contains(&c.id))
            .collect();

        let any_running = self.request_statuses.contains(&RequestStatus::Running);
        let all_done = self.is_finished;

        let course_items: Vec<ListItem> = courses_to_show
            .iter()
            .flat_map(|course| {
                let icon = if course.status == CourseStatus::Enrolled {
                    Span::styled("✅", Style::default().fg(Color::Green))
                } else if all_done {
                    Span::styled("❌", Style::default().fg(Color::Red))
                } else if any_running {
                    Span::styled(self.spinner.frame(), Style::default().fg(Color::Yellow))
                } else {
                    Span::styled("🕒", Style::default().fg(Color::DarkGray))
                };

                let mut items = vec![ListItem::new(Line::from(vec![
                    icon,
                    Span::raw(" "),
                    Span::raw(course.name.clone()),
                ]))];

                if let Some(errors) = self.course_errors.get(&course.id) {
                    for (error, attempt_idx) in errors {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("  └─ ⚠️ {}", error),
                                Style::default().fg(Color::Red),
                            ),
                            Span::raw(" "),
                            Span::styled(
                                format!("({}/{})", attempt_idx + 1, SIGNUP_ATTEMPTS),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ])));
                    }
                }
                items
            })
            .collect();

        let course_list = List::new(course_items).block(Block::default().title(" Kurse "));
        f.render_widget(course_list, chunks[2]);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

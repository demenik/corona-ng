use crate::app::{Course, CourseStatus, SIGNUP_ATTEMPTS};
use crate::ui::components::spinner::Spinner;
use chrono::{Local, NaiveTime};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table},
};

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
}

pub struct SignUpPopup {
    pub target_time: String,
    pub request_statuses: [RequestStatus; SIGNUP_ATTEMPTS],
    pub request_details: [Option<RequestDetail>; SIGNUP_ATTEMPTS],
    pub course_ids: Vec<String>,
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
        let popup_area = centered_rect(60, 60, area);
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
                Constraint::Length(3),                          // Countdown
                Constraint::Length(SIGNUP_ATTEMPTS as u16 + 3), // Requests (Header + rows + borders)
                Constraint::Min(0),                             // Courses
            ])
            .split(inner_area);

        // Countdown
        let countdown = self.calculate_countdown();
        f.render_widget(
            Paragraph::new(format!("Zeit bis Start: {}", countdown))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::BOTTOM))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            chunks[0],
        );

        // Requests
        let rows: Vec<Row> = self
            .request_statuses
            .iter()
            .enumerate()
            .map(|(i, status)| {
                let (icon, style) = match status {
                    RequestStatus::Waiting => ("🕒", Style::default().fg(Color::DarkGray)),
                    RequestStatus::Running => {
                        (self.spinner.frame(), Style::default().fg(Color::Yellow))
                    }
                    RequestStatus::Success => ("✅", Style::default().fg(Color::Green)),
                    RequestStatus::Failed => ("❌", Style::default().fg(Color::Red)),
                };

                let (s_text, f_text, local_text, server_text) =
                    if let Some(detail) = &self.request_details[i] {
                        (
                            detail.success_count.to_string(),
                            detail.failed_count.to_string(),
                            detail.local_time.clone(),
                            detail.server_time.clone().unwrap_or_default(),
                        )
                    } else {
                        (
                            "".to_string(),
                            "".to_string(),
                            "".to_string(),
                            "".to_string(),
                        )
                    };

                Row::new(vec![
                    Cell::from(icon),
                    Cell::from(format!("Anfrage #{}", i + 1)),
                    Cell::from(s_text).style(Style::default().fg(Color::Green)),
                    Cell::from(f_text).style(Style::default().fg(Color::Red)),
                    Cell::from(local_text),
                    Cell::from(server_text),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(2),  // Icon
                Constraint::Length(15), // Anfrage #
                Constraint::Length(4),  // Success
                Constraint::Length(4),  // Failed
                Constraint::Fill(1),    // Local Time
                Constraint::Fill(1),    // Server Time
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

        // Courses
        let courses_to_show: Vec<&Course> = courses
            .iter()
            .filter(|c| self.course_ids.contains(&c.id))
            .collect();

        let any_running = self.request_statuses.contains(&RequestStatus::Running);
        let all_done = self.is_finished;

        let course_items: Vec<ListItem> = courses_to_show
            .iter()
            .map(|course| {
                let icon = if course.status == CourseStatus::Enrolled {
                    Span::styled("✅", Style::default().fg(Color::Green))
                } else if all_done {
                    Span::styled("❌", Style::default().fg(Color::Red))
                } else if any_running {
                    Span::styled(self.spinner.frame(), Style::default().fg(Color::Yellow))
                } else {
                    Span::styled("🕒", Style::default().fg(Color::DarkGray))
                };

                ListItem::new(Line::from(vec![
                    icon,
                    Span::raw(" "),
                    Span::raw(course.name.clone()),
                ]))
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

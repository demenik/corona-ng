use chrono::{Local, Timelike};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{Component, ComponentAction};

pub struct TimeInput {
    pub course_id: String,
    pub hour: u32,
    pub minute: u32,
    pub is_hour_focused: bool,
}

impl TimeInput {
    pub fn new(course_id: String, initial_time: Option<&str>) -> Self {
        let (hour, minute) = if let Some(time_str) = initial_time {
            let parts: Vec<&str> = time_str.split(':').collect();
            if parts.len() == 2 {
                let h = parts[0].parse().unwrap_or(0);
                let m = parts[1].parse().unwrap_or(0);
                (h, m)
            } else {
                let now = Local::now();
                ((now.hour() + 1) % 24, 0)
            }
        } else {
            let now = Local::now();
            ((now.hour() + 1) % 24, 0)
        };

        Self {
            course_id,
            hour,
            minute,
            is_hour_focused: true,
        }
    }
}

impl Component for TimeInput {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ComponentAction> {
        let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
            15
        } else {
            1
        };

        match key.code {
            KeyCode::Left | KeyCode::Right => {
                self.is_hour_focused = !self.is_hour_focused;
                None
            }
            KeyCode::Up => {
                if self.is_hour_focused {
                    self.hour = (self.hour + step) % 24;
                } else {
                    self.minute = (self.minute + step) % 60;
                }
                None
            }
            KeyCode::Down => {
                if self.is_hour_focused {
                    self.hour = (self.hour + 24 - (step % 24)) % 24;
                } else {
                    self.minute = (self.minute + 60 - (step % 60)) % 60;
                }
                None
            }
            KeyCode::Enter => {
                let time_str = format!("{:02}:{:02}", self.hour, self.minute);
                Some(ComponentAction::SetSchedule(
                    self.course_id.clone(),
                    time_str,
                ))
            }
            _ => None,
        }
    }

    fn draw(&self, f: &mut Frame, area: Rect) {
        let popup_area = centered_rect(40, 15, area);
        f.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Zeit setzen ")
            .border_style(Style::default().fg(Color::Cyan));
        f.render_widget(block, popup_area);

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // Inputs
                Constraint::Min(0),    // Instructions
            ])
            .split(popup_area);

        let input_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Fill(1),
            ])
            .split(layout[0]);

        let hour_style = if self.is_hour_focused {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let minute_style = if !self.is_hour_focused {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        f.render_widget(
            Paragraph::new(format!("{:02}", self.hour))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" HH ")
                        .border_style(hour_style),
                ),
            input_chunks[0],
        );

        f.render_widget(
            Paragraph::new(":")
                .alignment(Alignment::Center)
                .style(Style::default().add_modifier(Modifier::BOLD)),
            input_chunks[1].inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
        );

        f.render_widget(
            Paragraph::new(format!("{:02}", self.minute))
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" MM ")
                        .border_style(minute_style),
                ),
            input_chunks[2],
        );

        let help_text = vec![
            Line::from(vec![
                Span::styled("Enter", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Bestätigen  "),
                Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Abbrechen"),
            ]),
            Line::from(vec![
                Span::styled("↑↓", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Ändern  "),
                Span::styled("Shift", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" ±15m  "),
                Span::styled("←→", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" Fokus"),
            ]),
        ];

        f.render_widget(
            Paragraph::new(help_text)
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            layout[1],
        );
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

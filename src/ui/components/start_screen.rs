use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

pub enum StartScreenAction {
    None,
    Login,
    OpenWeb,
    Quit,
}

pub struct StartScreen {
    selected_index: usize,
    buttons: Vec<&'static str>,
}

impl StartScreen {
    pub fn new() -> Self {
        Self {
            selected_index: 0,
            buttons: vec!["Login", "Webseite öffnen", "Verlassen"],
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> StartScreenAction {
        match key.code {
            KeyCode::Up => {
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                } else {
                    self.selected_index = self.buttons.len() - 1;
                }
                StartScreenAction::None
            }
            KeyCode::Down => {
                if self.selected_index < self.buttons.len() - 1 {
                    self.selected_index += 1;
                } else {
                    self.selected_index = 0;
                }
                StartScreenAction::None
            }
            KeyCode::Enter => match self.selected_index {
                0 => StartScreenAction::Login,
                1 => StartScreenAction::OpenWeb,
                2 => StartScreenAction::Quit,
                _ => StartScreenAction::None,
            },
            _ => StartScreenAction::None,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let vertical_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(10),
                Constraint::Length(2),
                Constraint::Length(9),
                Constraint::Min(0),
            ])
            .split(area);

        let center_chunk = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(72),
                Constraint::Min(0),
            ])
            .split(vertical_chunks[1]);

        let button_chunk = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(30),
                Constraint::Min(0),
            ])
            .split(vertical_chunks[3]);

        let ascii_logo = r#"
 ██████╗ ██████╗ ██████╗  ██████╗ ███╗   ██╗ █████╗ ███╗   ██╗ ██████╗ 
██╔════╝██╔═══██╗██╔══██╗██╔═══██╗████╗  ██║██╔══██╗████╗  ██║██╔════╝ 
██║     ██║   ██║██████╔╝██║   ██║██╔██╗ ██║███████║██╔██╗ ██║██║  ███╗
██║     ██║   ██║██╔══██╗██║   ██║██║╚██╗██║██╔══██║██║╚██╗██║██║   ██║
╚██████╗╚██████╔╝██║  ██║╚██████╔╝██║ ╚████║██║  ██║██║ ╚████║╚██████╔╝
 ╚═════╝ ╚═════╝ ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝╚═╝  ╚═╝╚═╝  ╚═══╝ ╚═════╝ 
██  ██ ▄▄▄▄▄ ▄▄    ▄▄▄▄  ▄▄▄▄▄ ▄▄▄▄ 
██████ ██▄▄  ██    ██▄█▀ ██▄▄  ██▄█▄
██  ██ ██▄▄▄ ██▄▄▄ ██    ██▄▄▄ ██ ██
        "#;

        let logo = Paragraph::new(ascii_logo)
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(logo, center_chunk[1]);

        let button_areas = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(button_chunk[1]);

        for (i, title) in self.buttons.iter().enumerate() {
            let is_selected = i == self.selected_index;

            let style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray).bg(Color::Reset)
            };

            let button = Paragraph::new(title.to_string())
                .alignment(Alignment::Center)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(if is_selected {
                            Style::default().fg(Color::Black)
                        } else {
                            Style::default()
                        })
                        .style(style),
                );

            f.render_widget(button, button_areas[i]);
        }
    }
}

impl Default for StartScreen {
    fn default() -> Self {
        Self::new()
    }
}

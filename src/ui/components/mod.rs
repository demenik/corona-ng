use crossterm::event::KeyEvent;
use ratatui::prelude::*;

use crate::app::CurrentScreen;

pub mod start_screen;

pub trait Component {
    fn handle_key(&mut self, key: KeyEvent) -> Option<ComponentAction>;

    fn draw(&self, f: &mut Frame, area: Rect);
}

pub enum ComponentAction {
    ChangeScreen(CurrentScreen),
    TriggerLogin(String, String),
    Quit,
}

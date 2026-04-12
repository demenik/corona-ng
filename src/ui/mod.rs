pub mod components;

use crate::{
    app::{App, CurrentScreen},
    ui::components::Component,
};
use ratatui::prelude::*;

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.current_screen {
        CurrentScreen::Start => {
            app.start_screen.draw(f, f.area());
        }
        CurrentScreen::Login => {
            app.login_screen.draw(f, f.area());
        }
        CurrentScreen::Dashboard => {
            let widget = ratatui::widgets::Paragraph::new("ESC");
            f.render_widget(widget, f.area());
        }
    }
}

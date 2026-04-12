use std::time::{SystemTime, UNIX_EPOCH};

pub struct Spinner {
    frames: Vec<&'static str>,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
        }
    }

    pub fn frame(&self) -> &'static str {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let index = (now / 100) as usize % self.frames.len();
        self.frames[index]
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

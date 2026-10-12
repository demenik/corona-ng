use std::{sync::RwLock, time::Instant};

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;

/// Time zone of the CoronaNG server. Schedule times are interpreted in this zone
pub const SERVER_TZ: Tz = chrono_tz::Europe::Berlin;

/// Process-wide clock shared by backend and UI
static CLOCK: RwLock<ServerClock> = RwLock::new(ServerClock::new());

/// Estimate of the CoronaNG server clock relative to the local system clock
#[derive(Debug, Clone, Copy)]
pub struct ServerClock {
    /// Server time minus local time. Positive if the server is ahead
    pub offset: Duration,
    /// Moment of the last successful sync, `None` if never synced
    pub synced_at: Option<Instant>,
    /// Number of samples the current offset is based on
    pub samples: usize,
}

impl ServerClock {
    pub const fn new() -> Self {
        Self {
            offset: Duration::zero(),
            synced_at: None,
            samples: 0,
        }
    }

    /// Estimated current server time in [`SERVER_TZ`]
    pub fn now(&self) -> DateTime<Tz> {
        (Utc::now() + self.offset).with_timezone(&SERVER_TZ)
    }
}

impl Default for ServerClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of the shared clock
pub fn get() -> ServerClock {
    *CLOCK.read().unwrap_or_else(|e| e.into_inner())
}

/// Replace the shared clock
pub fn set(clock: ServerClock) {
    *CLOCK.write().unwrap_or_else(|e| e.into_inner()) = clock;
}

/// Estimated current server time in [`SERVER_TZ`]
pub fn now() -> DateTime<Tz> {
    get().now()
}

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

/// Single offset measurement from one HTTP request
#[derive(Debug, Clone, Copy)]
pub struct OffsetSample {
    /// Server time minus local time at the request midpoint
    pub offset: Duration,
    /// Round-trip time until the response headers arrived
    pub rtt: Duration,
}

/// Offset estimate combined from several [`OffsetSample`]s
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OffsetEstimate {
    /// Server time minus local time. Positive if the server is ahead
    pub offset: Duration,
    /// Half the spread of the used samples
    pub uncertainty: Duration,
    /// Number of samples the estimate is based on
    pub samples: usize,
}

impl OffsetEstimate {
    /// Keep the fastest half of `samples` and take the median of their offsets.
    /// Returns `None` if `samples` is empty
    pub fn from_samples(mut samples: Vec<OffsetSample>) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }

        // Slow requests have a wide window in which the server may have read its clock
        samples.sort_by_key(|s| s.rtt);
        samples.truncate(samples.len().div_ceil(2));

        let mut offsets: Vec<Duration> = samples.iter().map(|s| s.offset).collect();
        offsets.sort();

        let mid = offsets.len() / 2;
        let offset = if offsets.len() % 2 == 0 {
            offsets[mid - 1] + (offsets[mid] - offsets[mid - 1]) / 2
        } else {
            offsets[mid]
        };
        let uncertainty = (offsets[offsets.len() - 1] - offsets[0]) / 2;

        Some(Self {
            offset,
            uncertainty,
            samples: offsets.len(),
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(offset_ms: i64, rtt_ms: i64) -> OffsetSample {
        OffsetSample {
            offset: Duration::milliseconds(offset_ms),
            rtt: Duration::milliseconds(rtt_ms),
        }
    }

    #[test]
    fn empty_samples_give_no_estimate() {
        assert_eq!(OffsetEstimate::from_samples(Vec::new()), None);
    }

    #[test]
    fn slow_samples_are_dropped() {
        let estimate = OffsetEstimate::from_samples(vec![
            sample(100, 20),
            sample(5000, 900),
            sample(300, 30),
            sample(-4000, 800),
        ])
        .unwrap();

        assert_eq!(estimate.samples, 2);
        assert_eq!(estimate.offset, Duration::milliseconds(200));
        assert_eq!(estimate.uncertainty, Duration::milliseconds(100));
    }

    #[test]
    fn odd_count_uses_middle_offset() {
        let estimate = OffsetEstimate::from_samples(vec![
            sample(-200, 10),
            sample(400, 10),
            sample(50, 10),
            sample(0, 50),
            sample(0, 60),
        ])
        .unwrap();

        assert_eq!(estimate.samples, 3);
        assert_eq!(estimate.offset, Duration::milliseconds(50));
        assert_eq!(estimate.uncertainty, Duration::milliseconds(300));
    }
}

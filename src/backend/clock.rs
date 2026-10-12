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

/// One request that read the server clock at whole-second resolution
#[derive(Debug, Clone, Copy)]
pub struct TimeProbe {
    /// Local time when the request was sent
    pub sent: DateTime<Utc>,
    /// Round-trip time until the server time was available
    pub rtt: Duration,
    /// Server time, truncated to whole seconds
    pub server: DateTime<Utc>,
}

impl TimeProbe {
    /// Local time when the server time was available
    pub fn received(&self) -> DateTime<Utc> {
        self.sent + self.rtt
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

impl From<TimeProbe> for OffsetSample {
    fn from(probe: TimeProbe) -> Self {
        // The server time is truncated to whole seconds, so the true time is on average 500ms later
        let offset = probe.server + Duration::milliseconds(500) - (probe.sent + probe.rtt / 2);
        Self {
            offset,
            rtt: probe.rtt,
        }
    }
}

/// Offset measured at the moment the server clock advanced to a new second
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundarySample {
    /// Server time minus local time
    pub offset: Duration,
    /// Half the width of the local time window that contains the second boundary
    pub uncertainty: Duration,
}

impl BoundarySample {
    /// Locate the server second boundary between two consecutive probes.
    /// Returns `None` if the server second did not advance
    pub fn between(before: &TimeProbe, after: &TimeProbe) -> Option<Self> {
        if after.server <= before.server {
            return None;
        }

        // `before` read its second before the boundary, so the boundary lies after `before.sent`.
        // `after` read its second after the boundary, so the boundary lies before `after.received()`
        let earliest = before.sent;
        let latest = after.received();
        let uncertainty = (latest - earliest) / 2;

        Some(Self {
            offset: after.server - (earliest + uncertainty),
            uncertainty,
        })
    }
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

        Some(Self {
            offset: median(&offsets),
            uncertainty: (offsets[offsets.len() - 1] - offsets[0]) / 2,
            samples: offsets.len(),
        })
    }

    /// Median of the boundary offsets. The uncertainty is the larger of the median
    /// boundary uncertainty and half the spread. Returns `None` if `samples` is empty
    pub fn from_boundaries(samples: &[BoundarySample]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }

        let mut offsets: Vec<Duration> = samples.iter().map(|s| s.offset).collect();
        offsets.sort();
        let mut uncertainties: Vec<Duration> = samples.iter().map(|s| s.uncertainty).collect();
        uncertainties.sort();

        let spread = (offsets[offsets.len() - 1] - offsets[0]) / 2;

        Some(Self {
            offset: median(&offsets),
            uncertainty: median(&uncertainties).max(spread),
            samples: samples.len(),
        })
    }
}

/// Median of a sorted, non-empty slice
fn median(sorted: &[Duration]) -> Duration {
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        sorted[mid - 1] + (sorted[mid] - sorted[mid - 1]) / 2
    } else {
        sorted[mid]
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

    fn probe(sent_ms: i64, rtt_ms: i64, server_s: i64) -> TimeProbe {
        TimeProbe {
            sent: DateTime::from_timestamp_millis(sent_ms).unwrap(),
            rtt: Duration::milliseconds(rtt_ms),
            server: DateTime::from_timestamp(server_s, 0).unwrap(),
        }
    }

    #[test]
    fn probe_sample_corrects_truncation() {
        let sample = OffsetSample::from(probe(10_000, 100, 12));
        assert_eq!(sample.offset, Duration::milliseconds(2450));
        assert_eq!(sample.rtt, Duration::milliseconds(100));
    }

    #[test]
    fn boundary_needs_second_change() {
        assert_eq!(
            BoundarySample::between(&probe(10_000, 30, 12), &probe(10_040, 30, 12)),
            None
        );
    }

    #[test]
    fn boundary_lies_between_probes() {
        // Boundary window is [10_000, 10_070], midpoint 10_035
        let sample =
            BoundarySample::between(&probe(10_000, 30, 12), &probe(10_040, 30, 13)).unwrap();
        assert_eq!(sample.offset, Duration::milliseconds(2965));
        assert_eq!(sample.uncertainty, Duration::milliseconds(35));
    }

    #[test]
    fn boundaries_use_median_and_widest_error() {
        let boundary = |offset_ms, uncertainty_ms| BoundarySample {
            offset: Duration::milliseconds(offset_ms),
            uncertainty: Duration::milliseconds(uncertainty_ms),
        };

        let tight = OffsetEstimate::from_boundaries(&[
            boundary(100, 30),
            boundary(110, 40),
            boundary(90, 20),
        ])
        .unwrap();
        assert_eq!(tight.offset, Duration::milliseconds(100));
        assert_eq!(tight.uncertainty, Duration::milliseconds(30));

        let spread = OffsetEstimate::from_boundaries(&[
            boundary(0, 10),
            boundary(100, 10),
            boundary(300, 10),
        ])
        .unwrap();
        assert_eq!(spread.offset, Duration::milliseconds(100));
        assert_eq!(spread.uncertainty, Duration::milliseconds(150));
    }
}

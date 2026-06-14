//! Committer-timestamp anomaly tracking.
//!
//! Counts committer timestamps that fall outside the sane analysis window.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// Lower bound for a plausible committer timestamp:
/// 1990-01-01T00:00:00Z as a unix timestamp in seconds (frozen sanitization
/// contract).
pub const MIN_SANE_COMMIT_TIME_UNIX: i64 = 631_152_000;

/// Upper-bound grace allowed past wall-clock time (24 hours), in seconds.
pub const MAX_CLOCK_SKEW_SECS: i64 = 24 * 60 * 60;

/// Counts committer-timestamp anomalies.
///
/// The counters are atomic so per-shard forks stay safe by construction, and
/// the tracker is shared via [`Arc`] so clones aggregate into one tally. The
/// counters are the only part observable through [`TimeAnomalyStats`]; any
/// warning log line is operational (not report) output, so its formatting is
/// not byte-identity-binding, but the counts are preserved exactly.
#[derive(Debug, Default)]
pub struct TimeAnomalyTracker {
    before_min: AtomicI64,
    after_max: AtomicI64,
}

impl TimeAnomalyTracker {
    /// Construct an empty tracker.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record a timestamp earlier than [`MIN_SANE_COMMIT_TIME_UNIX`].
    pub fn record_before_min(&self) {
        self.before_min.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a timestamp too far in the future.
    pub fn record_after_max(&self) {
        self.after_max.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot the running counts.
    #[must_use]
    pub fn snapshot(&self) -> TimeAnomalyStats {
        TimeAnomalyStats {
            before_min: self.before_min.load(Ordering::Relaxed),
            after_max: self.after_max.load(Ordering::Relaxed),
        }
    }
}

/// Anomalous committer-timestamp detections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeAnomalyStats {
    /// Count of timestamps earlier than 1990-01-01 UTC.
    pub before_min: i64,
    /// Count of timestamps more than `maxClockSkew` past wall-clock.
    pub after_max: i64,
}

impl TimeAnomalyStats {
    /// Combined count of anomalies on both bounds.
    #[must_use]
    pub const fn total(&self) -> i64 {
        self.before_min + self.after_max
    }
}

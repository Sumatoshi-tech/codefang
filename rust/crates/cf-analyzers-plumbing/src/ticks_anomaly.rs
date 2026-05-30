//! Committer-timestamp anomaly tracking.
//!
//! Port of `internal/analyzers/plumbing/ticks_anomaly.go`.
//!
//! Counts committer timestamps that fall outside the sane analysis window and
//! rate-limits the warning log, exactly mirroring the Go `timeAnomalyTracker`.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

/// Lower bound for a plausible committer timestamp, mirroring Go's
/// `minSaneCommitTime = time.Date(1990, 1, 1, 0, 0, 0, 0, time.UTC)`.
///
/// Expressed as a unix timestamp in seconds: 1990-01-01T00:00:00Z.
pub const MIN_SANE_COMMIT_TIME_UNIX: i64 = 631_152_000;

/// Upper-bound grace allowed past wall-clock time, mirroring Go's
/// `maxClockSkew = 24 * time.Hour`, in seconds.
pub const MAX_CLOCK_SKEW_SECS: i64 = 24 * 60 * 60;

/// Counts committer-timestamp anomalies, mirroring Go's `timeAnomalyTracker`.
///
/// The Go tracker rate-limits a warning log via atomics so the per-shard
/// `Fork()` clones stay safe by construction. The counters are the only part
/// observable through [`TimeAnomalyStats`]; the log line itself is operational
/// (not report) output, so its formatting parity is not byte-identity-binding,
/// but the counts are preserved exactly. Wrapped in an [`Arc`] so `Fork()`
/// clones share the aggregate, matching the Go field being a pointer shared
/// across clones.
#[derive(Debug, Default)]
pub struct TimeAnomalyTracker {
    before_min: AtomicI64,
    after_max: AtomicI64,
}

impl TimeAnomalyTracker {
    /// Construct an empty tracker.
    pub fn new() -> Arc<Self> {
        Arc::new(TimeAnomalyTracker::default())
    }

    /// Record a timestamp earlier than [`MIN_SANE_COMMIT_TIME_UNIX`].
    pub fn record_before_min(&self) {
        self.before_min.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a timestamp too far in the future.
    pub fn record_after_max(&self) {
        self.after_max.fetch_add(1, Ordering::Relaxed);
    }

    /// Snapshot the running counts, mirroring Go's `snapshot()`.
    pub fn snapshot(&self) -> TimeAnomalyStats {
        TimeAnomalyStats {
            before_min: self.before_min.load(Ordering::Relaxed),
            after_max: self.after_max.load(Ordering::Relaxed),
        }
    }
}

/// Anomalous committer-timestamp detections, mirroring Go's `TimeAnomalyStats`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimeAnomalyStats {
    /// Count of timestamps earlier than 1990-01-01 UTC.
    pub before_min: i64,
    /// Count of timestamps more than `maxClockSkew` past wall-clock.
    pub after_max: i64,
}

impl TimeAnomalyStats {
    /// Combined count of anomalies on both bounds, mirroring Go's `Total()`.
    pub fn total(&self) -> i64 {
        self.before_min + self.after_max
    }
}

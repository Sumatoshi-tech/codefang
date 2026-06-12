//! Injectable wall clock for the tick anomaly window.
//!
//! The tick sanitizer computes its upper bound as now + max clock skew. Per
//! DESIGN §2.8 this wall-clock read is a byte-identity hazard and must be
//! made deterministic through an injectable clock (the `CODEFANG_NOW`
//! pattern). This module provides that seam.

/// A source of "now" as a unix timestamp in seconds — the injectable clock
/// the design mandates for wall-clock reads (DESIGN §2.8).
pub trait Clock: Send + Sync {
    /// Current wall-clock time as whole seconds since the unix epoch (UTC).
    fn now_unix(&self) -> i64;
}

/// The real system clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }
}

/// A fixed clock for deterministic tests and reproducible goldens (the
/// `CODEFANG_NOW`/`SOURCE_DATE_EPOCH` seam).
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now_unix(&self) -> i64 {
        self.0
    }
}

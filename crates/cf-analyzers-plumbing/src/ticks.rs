//! `TicksSinceStart` provider.
//!
//! Assigns each commit an integer "tick" relative to the first commit's
//! tick-boundary-floored committer time. The tick is monotonically
//! non-decreasing (`max(tick, previous_tick)`), and committer timestamps are
//! sanitized into a sane window before the math runs. The wall-clock upper
//! bound (now + max clock skew) — a byte-identity hazard, per DESIGN §2.8 —
//! is read through an injectable [`Clock`] so goldens are reproducible.
//!
//! Tick assignment feeds the history analyzers' report keys; it is pinned by
//! the differential gate (`tests/compat`).

use std::collections::HashMap;
use std::sync::Arc;

use crate::analyzer::{dep, Analyzer, AnalyzerError, ValueMap};
use crate::clock::{Clock, SystemClock};
use crate::git_model::{Commit, Hash};
use crate::ticks_anomaly::{TimeAnomalyStats, TimeAnomalyTracker, MAX_CLOCK_SKEW_SECS, MIN_SANE_COMMIT_TIME_UNIX};

/// Default tick size in hours.
pub const DEFAULT_TICKS_SINCE_START_TICK_SIZE_HOURS: i64 = 24;

const SECS_PER_HOUR: i64 = 3600;

/// `TicksSinceStart` provider.
pub struct TicksSinceStart {
    /// Tick width in seconds, set from the configured hours.
    tick_size_secs: i64,
    /// The tick value of the last processed commit.
    pub tick: i64,
    /// Commit hashes grouped by tick.
    pub commits: HashMap<i64, Vec<Hash>>,
    previous_tick: i64,
    /// Tick origin, seeded lazily from the first in-window commit. `None`
    /// means "not yet seeded".
    tick0_unix: Option<i64>,
    /// Most recent in-window committer time; the substitution source for
    /// out-of-window timestamps. `None` until one has been seen.
    last_valid_when: Option<i64>,
    anomalies: Arc<TimeAnomalyTracker>,
    clock: Arc<dyn Clock>,
}

impl Default for TicksSinceStart {
    fn default() -> Self {
        Self::new()
    }
}

impl TicksSinceStart {
    /// Construct with the default tick size and the real system clock.
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Arc::new(SystemClock))
    }

    /// Construct with the default tick size and an injected clock (for
    /// deterministic goldens, per DESIGN §2.8).
    #[must_use]
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self {
            tick_size_secs: DEFAULT_TICKS_SINCE_START_TICK_SIZE_HOURS * SECS_PER_HOUR,
            tick: 0,
            commits: HashMap::new(),
            previous_tick: 0,
            tick0_unix: None,
            last_valid_when: None,
            anomalies: TimeAnomalyTracker::new(),
            clock,
        }
    }

    /// Set the tick size in hours (the `TicksSinceStart.TickSize`
    /// configuration value).
    pub fn set_tick_size_hours(&mut self, hours: i64) {
        self.tick_size_secs = hours * SECS_PER_HOUR;
    }

    /// Round a timestamp down to the nearest tick boundary.
    ///
    /// The reference floor rounds to the nearest multiple of the grid since
    /// an absolute zero time (away-from-zero on a tie) and subtracts one grid
    /// step if the rounded result moved past `t`. For non-negative unix
    /// seconds this equals arithmetic floor division onto the tick grid.
    #[must_use]
    pub const fn floor_time(when_unix: i64, tick_size_secs: i64) -> i64 {
        if tick_size_secs <= 0 {
            return when_unix;
        }
        // The reference floor is relative to an absolute zero time, but the
        // only use here is computing deltas `when - tick0` that are then
        // divided by the same grid, so flooring relative to the unix epoch is
        // equivalent for tick assignment. Euclidean floor handles negatives
        // the same way as the reference round-then-subtract.
        when_unix.div_euclid(tick_size_secs) * tick_size_secs
    }

    /// Clamp a committer timestamp into the sane window.
    ///
    /// Returns the (possibly substituted) timestamp and records anomalies.
    fn sanitize_when(&mut self, when_unix: i64) -> i64 {
        let upper_bound = self.clock.now_unix() + MAX_CLOCK_SKEW_SECS;
        if when_unix < MIN_SANE_COMMIT_TIME_UNIX {
            let replacement = self.substitute_when();
            self.anomalies.record_before_min();
            return replacement;
        }
        if when_unix > upper_bound {
            let replacement = self.substitute_when();
            self.anomalies.record_after_max();
            return replacement;
        }
        self.last_valid_when = Some(when_unix);
        when_unix
    }

    /// Pick a stand-in for an out-of-window committer time: the most recent
    /// in-window value, or the [`MIN_SANE_COMMIT_TIME_UNIX`] floor when none
    /// has been seen.
    fn substitute_when(&self) -> i64 {
        self.last_valid_when.unwrap_or(MIN_SANE_COMMIT_TIME_UNIX)
    }

    /// Compute the tick for one commit, advancing internal state.
    ///
    /// The algorithm (frozen; tick values flow into report keys):
    /// 1. sanitize the committer time;
    /// 2. on the first commit, seed `tick0 = floor_time(when, tick_size)`;
    /// 3. `tick = max((when - tick0) / tick_size, previous_tick)`;
    /// 4. accumulate the commit hash under `commits[tick]` (dedup tail-scan
    ///    for commits with parents);
    /// 5. record `tick`.
    pub fn tick_for(&mut self, committer_when_unix: i64, commit_hash: Hash, num_parents: usize) -> i64 {
        let when = self.sanitize_when(committer_when_unix);

        let tick0 = *self
            .tick0_unix
            .get_or_insert_with(|| Self::floor_time(when, self.tick_size_secs));

        // Integer division truncating toward zero (reference behavior), then
        // max with previous_tick.
        let delta = when - tick0;
        let raw_tick = delta / self.tick_size_secs;
        let tick = raw_tick.max(self.previous_tick);
        self.previous_tick = tick;

        let tick_commits = self.commits.entry(tick).or_default();
        // Scan the tail for an existing entry (dedup; root commits skip it).
        let exists = num_parents > 0 && tick_commits.iter().rev().any(|h| *h == commit_hash);
        if !exists {
            tick_commits.push(commit_hash);
        }

        self.tick = tick;
        tick
    }

    /// Cumulative committer-timestamp anomalies.
    #[must_use]
    pub fn time_anomalies(&self) -> TimeAnomalyStats {
        self.anomalies.snapshot()
    }

    /// Tick of the last processed commit.
    #[must_use]
    pub const fn current_tick(&self) -> i64 {
        self.tick
    }
}

impl Analyzer for TicksSinceStart {
    fn name(&self) -> &'static str {
        "TicksSinceStart"
    }

    fn provides(&self) -> Vec<&'static str> {
        vec!["tick"]
    }

    fn requires(&self) -> Vec<&'static str> {
        vec![]
    }

    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError> {
        let commit = dep::<Commit>(deps, "commit")?.clone();
        // num_parents is carried alongside the commit when present,
        // defaulting to 0.
        let num_parents = deps
            .get("num_parents")
            .and_then(|v| v.downcast_ref::<usize>())
            .copied()
            .unwrap_or(0);
        let commit_hash = deps
            .get("commit_hash")
            .and_then(|v| v.downcast_ref::<Hash>())
            .copied()
            .unwrap_or(Hash::ZERO);
        let tick = self.tick_for(commit.committer.when_unix, commit_hash, num_parents);
        let mut out = ValueMap::new();
        out.insert("tick".to_string(), Box::new(tick));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;

    const DAY: i64 = 24 * 60 * 60;
    // 2020-01-01T00:00:00Z as unix seconds (a tick boundary for 24h ticks).
    const START_2020: i64 = 1_577_836_800;
    // A fixed "now" far in the future so the upper bound never trips in tests.
    const NOW_2030: i64 = 1_893_456_000;

    fn fixed() -> TicksSinceStart {
        TicksSinceStart::with_clock(Arc::new(FixedClock(NOW_2030)))
    }

    fn h(n: u8) -> Hash {
        let mut b = [0u8; 20];
        b[0] = n;
        Hash(b)
    }

    // Mirrors the reference suite's first-tick expectation.
    #[test]
    fn first_tick_is_zero() {
        let mut ts = fixed();
        assert_eq!(ts.tick_for(START_2020, h(1), 0), 0);
    }

    #[test]
    fn multiple_ticks_two_days_later() {
        let mut ts = fixed();
        assert_eq!(ts.tick_for(START_2020, h(1), 0), 0);
        assert_eq!(ts.tick_for(START_2020 + 2 * DAY, h(2), 1), 2);
    }

    // The monotonic clamp: an earlier (in-window) commit cannot lower the tick.
    #[test]
    fn monotonic_clamp() {
        let mut ts = fixed();
        assert_eq!(ts.tick_for(START_2020 + 2 * DAY, h(1), 0), 0); // tick0 floor at +2d
        // Earlier but still in window -> raw negative, clamped to previousTick.
        assert_eq!(ts.tick_for(START_2020, h(2), 1), 0);
    }

    // Below the sane floor -> substituted, anomaly counted.
    #[test]
    fn before_min_is_substituted_and_counted() {
        let mut ts = fixed();
        // First a valid commit so lastValidWhen is set.
        ts.tick_for(START_2020 + DAY, h(1), 0);
        // Epoch-0 commit: before 1990 -> substituted with lastValidWhen.
        ts.tick_for(0, h(2), 1);
        assert_eq!(ts.time_anomalies().before_min, 1);
        assert_eq!(ts.time_anomalies().after_max, 0);
    }

    // Far future -> substituted, after-max counted.
    #[test]
    fn after_max_is_counted() {
        let mut ts = fixed();
        ts.tick_for(START_2020, h(1), 0);
        // Way past now + skew.
        ts.tick_for(NOW_2030 + 10 * DAY, h(2), 1);
        assert_eq!(ts.time_anomalies().after_max, 1);
    }

    #[test]
    fn floor_time_rounds_down_to_grid() {
        // Half a day past a boundary floors back to the boundary.
        assert_eq!(
            TicksSinceStart::floor_time(START_2020 + DAY / 2, DAY),
            START_2020
        );
        assert_eq!(TicksSinceStart::floor_time(START_2020, DAY), START_2020);
    }

    #[test]
    fn commits_accumulate_per_tick() {
        let mut ts = fixed();
        ts.tick_for(START_2020, h(1), 0);
        ts.tick_for(START_2020, h(2), 1);
        assert_eq!(ts.commits.get(&0).unwrap().len(), 2);
    }

    #[test]
    fn provider_metadata() {
        let ts = fixed();
        assert_eq!(ts.name(), "TicksSinceStart");
        assert_eq!(ts.provides(), vec!["tick"]);
        assert!(ts.requires().is_empty());
    }
}

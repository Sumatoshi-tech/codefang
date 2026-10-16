//! `HistoryTicks`: the ONE tick assignment every history walk uses.
//!
//! A tick is an integer bucket relative to the first in-window commit's
//! tick-boundary-floored committer time. The tick size comes from
//! `--tick-size` (hours, default [`DEFAULT_TICK_SIZE_HOURS`]). The tick is
//! monotonically non-decreasing over the walk (`max(tick, previous_tick)`), so
//! an out-of-order committer timestamp can never move a later commit into an
//! earlier bucket. Tick values flow into report keys (burndown bands, devs
//! activity, anomaly windows, `commits_by_tick`), so this is a report
//! contract.
//!
//! The assignment is a pure function of the commit times and the tick size —
//! no wall clock is consulted — so a run is reproducible on any machine on
//! any day.

/// Default tick size in hours (`--tick-size` default).
pub const DEFAULT_TICK_SIZE_HOURS: i64 = 24;

/// Seconds per hour.
const SECS_PER_HOUR: i64 = 3600;

/// The shared tick assigner for one history walk.
#[derive(Debug, Clone)]
pub struct HistoryTicks {
    tick_size_secs: i64,
    tick0_unix: Option<i64>,
    previous_tick: i64,
}

impl Default for HistoryTicks {
    fn default() -> Self {
        Self::new(DEFAULT_TICK_SIZE_HOURS)
    }
}

impl HistoryTicks {
    /// Construct with a tick size in hours. The CLI rejects a non-positive
    /// `--tick-size` before any walk starts; as a defensive floor a value
    /// below one hour is clamped to one hour so the arithmetic never divides
    /// by zero.
    #[must_use]
    pub fn new(tick_size_hours: i64) -> Self {
        Self {
            tick_size_secs: tick_size_hours.max(1) * SECS_PER_HOUR,
            tick0_unix: None,
            previous_tick: 0,
        }
    }

    /// The configured tick size in hours.
    #[must_use]
    pub const fn tick_size_hours(&self) -> i64 {
        self.tick_size_secs / SECS_PER_HOUR
    }

    /// Round a unix timestamp down to the nearest tick boundary.
    #[must_use]
    pub const fn floor_time(when_unix: i64, tick_size_secs: i64) -> i64 {
        if tick_size_secs <= 0 {
            return when_unix;
        }
        when_unix.div_euclid(tick_size_secs) * tick_size_secs
    }

    /// Compute the tick for one commit (committer time, unix seconds),
    /// advancing internal state:
    /// 1. on the first commit, seed `tick0 = floor_time(when, tick_size)`;
    /// 2. `tick = max(floor((when - tick0) / tick_size), previous_tick)`.
    pub fn tick_for(&mut self, committer_when_unix: i64) -> i64 {
        let tick0 = *self
            .tick0_unix
            .get_or_insert_with(|| Self::floor_time(committer_when_unix, self.tick_size_secs));
        let raw_tick = (committer_when_unix - tick0).div_euclid(self.tick_size_secs);
        let tick = raw_tick.max(self.previous_tick);
        self.previous_tick = tick;
        tick
    }

    /// Tick of the last processed commit (0 before any commit).
    #[must_use]
    pub const fn last_tick(&self) -> i64 {
        self.previous_tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3600;
    const DAY: i64 = 24 * HOUR;
    // 2020-01-01T00:00:00Z as unix seconds (a tick boundary for 24h ticks).
    const START_2020: i64 = 1_577_836_800;

    #[test]
    fn first_tick_is_zero_and_days_advance() {
        let mut ts = HistoryTicks::default();
        assert_eq!(ts.tick_size_hours(), DEFAULT_TICK_SIZE_HOURS);
        assert_eq!(ts.tick_for(START_2020), 0);
        assert_eq!(ts.tick_for(START_2020 + 2 * DAY), 2);
        assert_eq!(ts.last_tick(), 2);
    }

    /// The monotonic clamp: an earlier commit cannot lower the tick.
    #[test]
    fn monotonic_clamp() {
        let mut ts = HistoryTicks::default();
        assert_eq!(ts.tick_for(START_2020 + 2 * DAY), 0);
        assert_eq!(ts.tick_for(START_2020), 0);
        assert_eq!(ts.tick_for(START_2020 + 3 * DAY), 1);
    }

    /// The documented `--tick-size 1` contract over the ticks fixture:
    /// 2020-01-01T23:59Z, 2020-01-02T00:01Z, 2020-01-02T12:00+02:00,
    /// 2020-01-03T01:00-05:00, 2020-01-05T00:00Z ⇒ `[0, 1, 11, 31, 73]`.
    #[test]
    fn one_hour_ticks_follow_the_documented_fixture() {
        let times = [
            START_2020 + 23 * HOUR + 59 * 60,
            START_2020 + DAY + 60,
            START_2020 + DAY + 10 * HOUR,
            START_2020 + 2 * DAY + 6 * HOUR,
            START_2020 + 4 * DAY,
        ];
        let mut ts = HistoryTicks::new(1);
        let ticks: Vec<i64> = times.iter().map(|t| ts.tick_for(*t)).collect();
        assert_eq!(ticks, vec![0, 1, 11, 31, 73]);
        // The same instants under the default 24h tick.
        let mut daily = HistoryTicks::default();
        let ticks: Vec<i64> = times.iter().map(|t| daily.tick_for(*t)).collect();
        assert_eq!(ticks, vec![0, 1, 1, 2, 4]);
    }

    #[test]
    fn floor_time_rounds_down_to_grid() {
        assert_eq!(
            HistoryTicks::floor_time(START_2020 + DAY / 2, DAY),
            START_2020
        );
        assert_eq!(HistoryTicks::floor_time(START_2020, DAY), START_2020);
        assert_eq!(HistoryTicks::floor_time(-1, DAY), -DAY);
        assert_eq!(HistoryTicks::floor_time(42, 0), 42);
    }

    #[test]
    fn non_positive_tick_size_is_clamped_to_one_hour() {
        let ts = HistoryTicks::new(0);
        assert_eq!(ts.tick_size_hours(), 1);
        let ts = HistoryTicks::new(-5);
        assert_eq!(ts.tick_size_hours(), 1);
    }
}

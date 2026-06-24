//! Run provenance metadata.
//!
//! Defines [`AnalysisMetadata`] and [`new_analysis_metadata`].
//!
//! # The `analyzed_at` byte-identity hazard
//!
//! The reference binary stamps `analyzed_at` with the current RFC3339 UTC
//! time. Against a live wall clock this makes report bytes
//! non-deterministic, defeating the project's byte-identity goal
//! (DESIGN §2.8). This port therefore reads the time from an **injectable
//! clock** which honors the `CODEFANG_NOW` / `SOURCE_DATE_EPOCH` environment
//! overrides used by the golden harness, and formats it via the contract
//! RFC3339 formatter (third-party formatters differ on `Z` vs `+00:00` and on
//! fractional-second trimming, so the fixed-width second-precision form is
//! formatted here directly).

use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// Provenance information for a codefang run.
///
/// Run metadata stamped into reports. Field declaration order and JSON
/// (`json:"..."`) / YAML (`yaml:"..."`) tags follow the report contract:
/// `repo_path`, `repo_name`, `analyzed_at`, `codefang_version`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnalysisMetadata {
    /// `repo_path` — the analyzed repository path.
    pub repo_path: String,
    /// `repo_name` — `filepath.Base(repo_path)`.
    pub repo_name: String,
    /// `analyzed_at` — RFC3339 UTC timestamp of the run.
    pub analyzed_at: String,
    /// `codefang_version` — the codefang version string.
    pub codefang_version: String,
}

/// An injectable wall clock. The default ([`SystemClock`]) reads
/// `time.Now().UTC().Format(time.RFC3339)`; tests/golden runs substitute a fixed
/// clock so the `analyzed_at` envelope field is reproducible (DESIGN §2.8).
pub trait Clock {
    /// Returns the current time as contract RFC3339 UTC
    /// (`YYYY-MM-DDTHH:MM:SSZ`).
    fn now_rfc3339_utc(&self) -> String;
}

/// The production [`Clock`]: reads the resolved current time (honoring the
/// pinned clock and `CODEFANG_NOW`/`SOURCE_DATE_EPOCH` overrides) and formats it
/// as contract RFC3339 UTC.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339_utc(&self) -> String {
        format_rfc3339_utc(clock_now_unix_secs())
    }
}

impl AnalysisMetadata {
    /// Creates metadata for `repo_path`, stamping `analyzed_at` from the supplied
    /// [`Clock`]. Equivalent to [`new_analysis_metadata`] but with the time
    /// source injected (used by tests/golden runs for determinism).
    pub fn with_clock(repo_path: &str, clock: &dyn Clock) -> Self {
        Self {
            repo_path: repo_path.to_string(),
            repo_name: base_name(repo_path),
            analyzed_at: clock.now_rfc3339_utc(),
            codefang_version: cf_version::VERSION.to_string(),
        }
    }

    /// Builds the wrapper [`cf_gojson::GoValue`] in Go struct declaration order
    /// (`repo_path`, `repo_name`, `analyzed_at`, `codefang_version`), matching the
    /// `json:"..."` tags. Struct-origin so fields keep declaration order.
    #[must_use]
    pub fn to_go_value(&self) -> cf_gojson::GoValue {
        use cf_gojson::{GoMap, GoValue, MapOrigin};
        let mut m = GoMap::new(MapOrigin::Struct);
        m.insert("repo_path", GoValue::Str(self.repo_path.clone()));
        m.insert("repo_name", GoValue::Str(self.repo_name.clone()));
        m.insert("analyzed_at", GoValue::Str(self.analyzed_at.clone()));
        m.insert(
            "codefang_version",
            GoValue::Str(self.codefang_version.clone()),
        );
        GoValue::Map(m)
    }
}

/// Creates metadata for the given repository path.
///
/// `repo_name` is the final path component (`filepath.Base`), `analyzed_at` is
/// the current clock time in contract RFC3339 UTC, and `codefang_version`
/// is [`cf_version::VERSION`].
#[must_use]
pub fn new_analysis_metadata(repo_path: &str) -> AnalysisMetadata {
    AnalysisMetadata {
        repo_path: repo_path.to_string(),
        repo_name: base_name(repo_path),
        analyzed_at: format_rfc3339_utc(clock_now_unix_secs()),
        codefang_version: cf_version::VERSION.to_string(),
    }
}

/// Path base-name helper for the cases relevant here (returns the
/// final path element; `.` for empty input).
fn base_name(path: &str) -> String {
    if path.is_empty() {
        return ".".to_string();
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    match trimmed.rsplit_once('/') {
        Some((_, last)) => last.to_string(),
        None => trimmed.to_string(),
    }
}

/// Overridable clock for tests / reproducible goldens. When unset, the current
/// time is read from `CODEFANG_NOW`/`SOURCE_DATE_EPOCH` (if present) or the
/// system clock.
static FIXED_NOW: RwLock<Option<i64>> = RwLock::new(None);

/// Pins the clock to `unix_secs` and returns a restore guard. Dropping the
/// guard restores the previous setting on drop.
#[must_use = "dropping the guard restores the previous clock"]
pub fn set_fixed_now(unix_secs: i64) -> ClockRestore {
    let mut g = FIXED_NOW.write().expect("clock lock poisoned");
    let prev = *g;
    *g = Some(unix_secs);
    ClockRestore { prev: Some(prev) }
}

/// Restore guard returned by [`set_fixed_now`].
pub struct ClockRestore {
    prev: Option<Option<i64>>,
}

impl Drop for ClockRestore {
    fn drop(&mut self) {
        if let Some(prev) = self.prev.take() {
            let mut g = FIXED_NOW.write().expect("clock lock poisoned");
            *g = prev;
        }
    }
}

/// Resolves the current time as Unix seconds, honoring the pinned clock then
/// `CODEFANG_NOW` (RFC3339 or Unix seconds) and `SOURCE_DATE_EPOCH`.
fn clock_now_unix_secs() -> i64 {
    if let Some(secs) = *FIXED_NOW.read().expect("clock lock poisoned") {
        return secs;
    }
    if let Ok(v) = std::env::var("CODEFANG_NOW") {
        if let Some(secs) = parse_env_time(&v) {
            return secs;
        }
    }
    if let Ok(v) = std::env::var("SOURCE_DATE_EPOCH") {
        if let Ok(secs) = v.trim().parse::<i64>() {
            return secs;
        }
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parses `CODEFANG_NOW` as either Unix seconds or `YYYY-MM-DDTHH:MM:SSZ`.
fn parse_env_time(v: &str) -> Option<i64> {
    let v = v.trim();
    if let Ok(secs) = v.parse::<i64>() {
        return Some(secs);
    }
    parse_rfc3339_utc_secs(v)
}

/// Days from the proleptic-Gregorian civil date to the Unix epoch. Used by both
/// the formatter and parser. Algorithm from Howard Hinnant's `days_from_civil`.
const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Inverse of [`days_from_civil`] — civil date from days since the Unix epoch.
const fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Formats Unix seconds as contract RFC3339 UTC: `YYYY-MM-DDTHH:MM:SSZ`.
///
/// RFC3339 contract format: zero-padded fields, `T`
/// separator, and a literal `Z` zone for UTC (never `+00:00`). No
/// fractional seconds are emitted (RFC3339, second precision).
#[must_use]
pub fn format_rfc3339_utc(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86400);
    let secs_of_day = unix_secs.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hh, mm, ss)
}

/// Parses `YYYY-MM-DDTHH:MM:SSZ` (UTC, second precision) into Unix seconds.
/// Returns `None` on malformed input. Used only for the `CODEFANG_NOW` override.
fn parse_rfc3339_utc_secs(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T' {
        return None;
    }
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let y = num(0, 4)?;
    let m = num(5, 7)?;
    let d = num(8, 10)?;
    let hh = num(11, 13)?;
    let mm = num(14, 16)?;
    let ss = num(17, 19)?;
    let days = days_from_civil(y, m, d);
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize tests that mutate the global pinned clock.
    static CLOCK_GUARD: Mutex<()> = Mutex::new(());

    // Mirrors reference test TestNewAnalysisMetadata.
    #[test]
    fn new_analysis_metadata_fields() {
        let _g = CLOCK_GUARD.lock().unwrap();
        // Pin so AnalyzedAt is deterministic and parseable.
        let _r = set_fixed_now(1_704_067_200); // 2024-01-01T00:00:00Z
        let meta = new_analysis_metadata("/path/to/repo");
        assert_eq!(meta.repo_path, "/path/to/repo");
        assert_eq!(meta.repo_name, "repo");
        assert_eq!(meta.analyzed_at, "2024-01-01T00:00:00Z");
    }

    #[test]
    fn base_name_cases() {
        assert_eq!(base_name("/path/to/repo"), "repo");
        assert_eq!(base_name("repo"), "repo");
        assert_eq!(base_name("/path/to/repo/"), "repo");
        assert_eq!(base_name(""), ".");
    }

    // RFC3339 formatting parity with the reference binary (Z zone, no fraction).
    #[test]
    fn rfc3339_format_epoch() {
        assert_eq!(format_rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_rfc3339_utc(1_704_067_200), "2024-01-01T00:00:00Z");
        assert_eq!(format_rfc3339_utc(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn rfc3339_roundtrip() {
        let s = "2024-06-30T12:34:56Z";
        let secs = parse_rfc3339_utc_secs(s).unwrap();
        assert_eq!(format_rfc3339_utc(secs), s);
    }
}

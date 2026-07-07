//! Build-time injection of version metadata.
//!
//! The build script reads environment variables and re-exports them as
//! `cargo:rustc-env` values that the library picks up with `option_env!`.
//! When an env var is absent the library falls back to the frozen defaults,
//! so a plain `cargo build` with no env produces `dev` / `none` / `unknown`.
//!
//! Recognized inputs (first non-empty wins per field):
//!   Version: `CF_VERSION`, then `GIT_VERSION`
//!   Commit:  `CF_COMMIT`,  then `GIT_COMMIT`
//!   Date:    `CF_DATE`,    then `SOURCE_DATE_EPOCH` (epoch seconds -> RFC3339 UTC)
//!
//! `SOURCE_DATE_EPOCH` support makes `built:` reproducible (per DESIGN.md
//! §2.8): goldens pin it so the printed `built` date is stable.

use std::env;

/// Seconds in a (non-leap) day.
const SECS_PER_DAY: i64 = 86_400;
/// Seconds in an hour.
const SECS_PER_HOUR: i64 = 3_600;
/// Seconds in a minute.
const SECS_PER_MINUTE: i64 = 60;
/// Days from year 0 to the Unix epoch (1970-01-01) in the proleptic Gregorian
/// calendar; used by the civil-from-days conversion.
const DAYS_TO_UNIX_EPOCH: i64 = 719_468;
/// Days in the 400-year Gregorian cycle.
const DAYS_PER_ERA: i64 = 146_097;

fn first_env(keys: &[&str]) -> Option<String> {
    for k in keys {
        println!("cargo:rerun-if-env-changed={k}");
        if let Ok(v) = env::var(k) {
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

fn main() {
    // Re-run when any recognized input changes.
    let version = first_env(&["CF_VERSION", "GIT_VERSION"]);
    let commit = first_env(&["CF_COMMIT", "GIT_COMMIT"]);

    // Date: explicit CF_DATE wins; otherwise derive RFC3339 UTC from
    // SOURCE_DATE_EPOCH for reproducible builds.
    let date = first_env(&["CF_DATE"]).or_else(|| {
        first_env(&["SOURCE_DATE_EPOCH"])
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map(rfc3339_utc_from_epoch)
    });

    if let Some(v) = version {
        println!("cargo:rustc-env=CF_VERSION_INJECTED={v}");
    }
    if let Some(c) = commit {
        println!("cargo:rustc-env=CF_COMMIT_INJECTED={c}");
    }
    if let Some(d) = date {
        println!("cargo:rustc-env=CF_DATE_INJECTED={d}");
    }
}

/// Format a Unix epoch (seconds, UTC) as an RFC3339 timestamp with a `Z`
/// zone, the frozen rendering used for build dates. No fractional seconds.
fn rfc3339_utc_from_epoch(epoch: i64) -> String {
    let days = epoch.div_euclid(SECS_PER_DAY);
    let secs_of_day = epoch.rem_euclid(SECS_PER_DAY);
    let hour = secs_of_day / SECS_PER_HOUR;
    let minute = (secs_of_day % SECS_PER_HOUR) / SECS_PER_MINUTE;
    let second = secs_of_day % SECS_PER_MINUTE;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Convert a count of days since the Unix epoch to a civil (year, month, day)
/// in the proleptic Gregorian calendar. Algorithm by Howard Hinnant
/// (`days_from_civil` inverse), valid across the full i64 range we care about.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + DAYS_TO_UNIX_EPOCH;
    let era = if z >= 0 { z } else { z - (DAYS_PER_ERA - 1) } / DAYS_PER_ERA;
    let doe = z - era * DAYS_PER_ERA; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

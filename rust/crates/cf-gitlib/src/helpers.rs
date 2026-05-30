//! Repository loading and time parsing, ported from `pkg/gitlib/helpers.go`.
//!
//! [`parse_time`] accepts a Go-style duration (`"24h"`), an RFC3339 timestamp,
//! or a date-only string, returning **Unix epoch seconds** (the comparable
//! instant the rest of gitlib uses). [`load_repository`] rejects remote URIs;
//! [`load_commits`] loads commits with limit / first-parent / head-only / since
//! options, reversing to oldest-first like Go.
//!
//! # Clock injection
//!
//! Durations are resolved relative to "now". Per the design's wall-clock
//! neutralization (§2.8), the reference instant honors the `CODEFANG_NOW`
//! environment variable (RFC3339 or epoch seconds) when set, falling back to the
//! system clock — so duration-relative goldens are reproducible. This mirrors
//! Go's `time.Now()` while making it injectable.

use std::time::{SystemTime, UNIX_EPOCH};

use cf_alg::collect_n;

use crate::commit::Commit;
use crate::error::{GitError, Result};
use crate::repository::{LogOptions, Repository};

/// Options controlling how commits are loaded (Go `gitlib.CommitLoadOptions`).
#[derive(Debug, Clone, Default)]
pub struct CommitLoadOptions {
    /// Maximum number of commits to load (`0` = no limit).
    pub limit: i32,
    /// Follow only the first parent.
    pub first_parent: bool,
    /// Load only the HEAD commit.
    pub head_only: bool,
    /// `--since` time/ref spec (empty = no filter).
    pub since: String,
}

/// Number of seconds in a minute / hour / day for duration parsing.
const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 3600;

/// Opens a local git repository, rejecting remote URIs (Go `LoadRepository`).
///
/// Returns an error for `scheme://` URIs and `user@host:` SCP-style remotes.
/// Trailing path separators are trimmed, matching Go. Unlike Go (which calls
/// `log.Fatalf` on open failure), this returns the error so callers can handle
/// it — the Go fatal-exit behavior is a CLI concern reproduced at the binary
/// layer, not in the library.
///
/// # Errors
///
/// Returns [`GitError::RemoteNotSupported`] for remote URIs, or the open error.
pub fn load_repository(uri: &str) -> Result<Repository> {
    if uri.contains("://") || is_scp_style_remote(uri) {
        return Err(GitError::RemoteNotSupported(uri.to_string()));
    }

    let trimmed = uri.strip_suffix(std::path::MAIN_SEPARATOR).unwrap_or(uri);
    Repository::open(trimmed)
}

/// Reports whether `uri` looks like an SCP-style remote (`user@host:path`).
///
/// Mirrors Go's regex `^[A-Za-z]\w*@[A-Za-z0-9][\w.]*:`.
fn is_scp_style_remote(uri: &str) -> bool {
    let bytes = uri.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    // [A-Za-z]\w* up to '@'
    let mut i = 1;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'@' {
        return false;
    }
    i += 1;
    // [A-Za-z0-9]
    if i >= bytes.len() || !bytes[i].is_ascii_alphanumeric() {
        return false;
    }
    i += 1;
    // [\w.]* then ':'
    while i < bytes.len() {
        let c = bytes[i];
        if c == b':' {
            return true;
        }
        if !(c.is_ascii_alphanumeric() || c == b'_' || c == b'.') {
            return false;
        }
        i += 1;
    }
    false
}

/// Parses a time string to Unix epoch seconds (Go `ParseTime`).
///
/// Accepts, in order:
/// 1. a Go-style duration (e.g. `"24h"`, `"90m"`, `"1h30m"`) → `now - duration`;
/// 2. an RFC3339 timestamp (e.g. `"2024-01-01T00:00:00Z"`);
/// 3. a date-only string (e.g. `"2024-01-01"`, interpreted as midnight UTC).
///
/// # Errors
///
/// Returns [`GitError::InvalidTimeFormat`] when none of the formats match.
pub fn parse_time(s: &str) -> Result<i64> {
    if let Some(d) = parse_go_duration_secs(s) {
        return Ok(now_secs() - d);
    }
    if let Some(t) = parse_rfc3339(s) {
        return Ok(t);
    }
    if let Some(t) = parse_date_only(s) {
        return Ok(t);
    }
    Err(GitError::InvalidTimeFormat(s.to_string()))
}

/// Returns the reference "now" in epoch seconds, honoring `CODEFANG_NOW`.
fn now_secs() -> i64 {
    if let Ok(v) = std::env::var("CODEFANG_NOW") {
        if let Ok(secs) = v.parse::<i64>() {
            return secs;
        }
        if let Some(secs) = parse_rfc3339(&v) {
            return secs;
        }
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Parses a Go-style duration string into seconds (subset: `h`, `m`, `s` units).
///
/// Go's `time.ParseDuration` supports `ns`/`us`/`ms`/`s`/`m`/`h`; gitlib's
/// `--since` durations in practice use `h`/`m`/`s`, which this parser covers,
/// including compound values like `"1h30m"`. Returns `None` for non-durations.
fn parse_go_duration_secs(s: &str) -> Option<i64> {
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut total: i64 = 0;
    let mut saw_unit = false;

    // Optional leading sign.
    let neg = bytes[0] == b'-';
    if neg || bytes[0] == b'+' {
        i += 1;
    }

    while i < bytes.len() {
        // Parse the numeric magnitude (integer only; fractional durations are
        // uncommon for --since and would fall through to the date parsers).
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None; // expected a number before a unit
        }
        let num: i64 = s[start..i].parse().ok()?;

        // Parse the unit.
        if i >= bytes.len() {
            return None; // trailing number without a unit
        }
        let (mult, consumed) = match &bytes[i..] {
            [b'h', ..] => (SECONDS_PER_HOUR, 1),
            [b'm', b's', ..] => return None, // milliseconds: not seconds-granular
            [b'm', ..] => (SECONDS_PER_MINUTE, 1),
            [b's', ..] => (1, 1),
            _ => return None,
        };
        total = total.checked_add(num.checked_mul(mult)?)?;
        saw_unit = true;
        i += consumed;
    }

    if !saw_unit {
        return None;
    }
    Some(if neg { -total } else { total })
}

/// Parses an RFC3339 timestamp to epoch seconds (the `Z`/offset forms Go's
/// `time.RFC3339` accepts). Returns `None` if it is not RFC3339.
fn parse_rfc3339(s: &str) -> Option<i64> {
    // Format: YYYY-MM-DDThh:mm:ss(.fff)?(Z|±hh:mm)
    let bytes = s.as_bytes();
    if bytes.len() < 20 {
        return None;
    }
    if bytes[10] != b'T' && bytes[10] != b't' {
        return None;
    }
    let date = parse_ymd(&s[..10])?;
    let hh: i64 = s.get(11..13)?.parse().ok()?;
    if bytes.get(13) != Some(&b':') {
        return None;
    }
    let mm: i64 = s.get(14..16)?.parse().ok()?;
    if bytes.get(16) != Some(&b':') {
        return None;
    }
    let ss: i64 = s.get(17..19)?.parse().ok()?;

    // Skip optional fractional seconds.
    let mut idx = 19;
    if bytes.get(idx) == Some(&b'.') {
        idx += 1;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
    }

    // Timezone.
    let offset_secs = match bytes.get(idx) {
        Some(b'Z') | Some(b'z') => 0,
        Some(b'+') | Some(b'-') => {
            let sign = if bytes[idx] == b'-' { -1 } else { 1 };
            let oh: i64 = s.get(idx + 1..idx + 3)?.parse().ok()?;
            // Accept "+hh:mm" or "+hhmm".
            let om: i64 = if bytes.get(idx + 3) == Some(&b':') {
                s.get(idx + 4..idx + 6)?.parse().ok()?
            } else {
                s.get(idx + 3..idx + 5)?.parse().ok()?
            };
            sign * (oh * SECONDS_PER_HOUR + om * SECONDS_PER_MINUTE)
        }
        _ => return None,
    };

    let day_secs = hh * SECONDS_PER_HOUR + mm * SECONDS_PER_MINUTE + ss;
    Some(date + day_secs - offset_secs)
}

/// Parses a date-only `YYYY-MM-DD` string to epoch seconds at midnight UTC
/// (Go `time.DateOnly`). Returns `None` if it is not a plain date.
fn parse_date_only(s: &str) -> Option<i64> {
    if s.len() != 10 {
        return None;
    }
    parse_ymd(s)
}

/// Parses `YYYY-MM-DD` to epoch seconds at midnight UTC.
fn parse_ymd(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400)
}

/// Days from the Unix epoch for a (proleptic Gregorian) civil date.
///
/// Howard Hinnant's `days_from_civil` algorithm (public domain), the standard
/// branchless conversion; equivalent to what Go's `time.Date(...).Unix()/86400`
/// produces for UTC dates.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Loads commits from a repository per `opts` (Go `LoadCommits`).
///
/// For `head_only`, returns just the HEAD commit. Otherwise walks history with
/// the given limit / first-parent / since options, then **reverses** so the
/// result is oldest-first (matching Go's `slices.Reverse`).
///
/// # Errors
///
/// Returns HEAD/lookup/log errors, or [`GitError::InvalidTimeFormat`] when
/// `--since` cannot be resolved.
pub fn load_commits<'repo>(
    repository: &'repo Repository,
    opts: &CommitLoadOptions,
) -> Result<Vec<Commit<'repo>>> {
    if opts.head_only {
        return load_head_commit(repository);
    }
    load_history_commits(repository, opts)
}

fn load_head_commit(repository: &Repository) -> Result<Vec<Commit<'_>>> {
    let head_hash = repository.head()?;
    let commit = repository.lookup_commit(head_hash)?;
    Ok(vec![commit])
}

fn load_history_commits<'repo>(
    repository: &'repo Repository,
    opts: &CommitLoadOptions,
) -> Result<Vec<Commit<'repo>>> {
    let mut log_opts = LogOptions {
        first_parent: opts.first_parent,
        ..Default::default()
    };

    if !opts.since.is_empty() {
        let secs = repository.resolve_time(&opts.since)?;
        log_opts.since = Some(git2::Time::new(secs, 0));
    }

    let mut iter = repository.log(&log_opts)?;
    // CollectN drains up to `limit` (all when limit == 0), exactly like Go's
    // CollectN convention. A negative Go limit is treated as 0 (unlimited),
    // matching `alg.CollectN` where `limit <= 0` collects everything.
    let limit = if opts.limit > 0 {
        opts.limit as usize
    } else {
        0
    };
    let mut commits = collect_n(&mut iter, limit)
        .expect("CommitIter::next never yields a non-EOF error");
    commits.reverse();
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_only_works() {
        // 2024-01-01T00:00:00Z == 1704067200.
        assert_eq!(parse_time("2024-01-01").unwrap(), 1_704_067_200);
    }

    #[test]
    fn parse_rfc3339_z() {
        assert_eq!(
            parse_time("2024-01-01T00:00:00Z").unwrap(),
            1_704_067_200
        );
    }

    #[test]
    fn parse_rfc3339_offset() {
        // 2024-01-01T01:00:00+01:00 == 2024-01-01T00:00:00Z.
        assert_eq!(
            parse_time("2024-01-01T01:00:00+01:00").unwrap(),
            1_704_067_200
        );
    }

    #[test]
    fn parse_duration_relative() {
        std::env::set_var("CODEFANG_NOW", "1000000");
        assert_eq!(parse_time("24h").unwrap(), 1_000_000 - 24 * 3600);
        assert_eq!(parse_time("1h30m").unwrap(), 1_000_000 - (3600 + 1800));
        std::env::remove_var("CODEFANG_NOW");
    }

    #[test]
    fn parse_invalid() {
        assert!(matches!(
            parse_time("not-a-time"),
            Err(GitError::InvalidTimeFormat(_))
        ));
    }

    #[test]
    fn load_repository_rejects_remotes() {
        assert!(matches!(
            load_repository("https://github.com/x/y.git"),
            Err(GitError::RemoteNotSupported(_))
        ));
        assert!(matches!(
            load_repository("git@github.com:x/y.git"),
            Err(GitError::RemoteNotSupported(_))
        ));
    }

    #[test]
    fn days_from_civil_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2024, 1, 1), 19_723);
    }
}

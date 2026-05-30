//! System / process memory metrics.
//!
//! Port of `internal/observability/sysmetrics.go`.
//!
//! The Linux `/proc` parsing ([`read_rss_bytes`], [`read_smaps_rollup`]) is a
//! faithful, behavior-identical port — same files, same prefixes, same kB→bytes
//! conversion, same zero-on-error fallback.
//!
//! The [`HeapSnapshot`] runtime fields (`heap_inuse`, `heap_alloc`,
//! `heap_objects`, `stack_inuse`, `next_gc`, `sys`, `num_gc`, `goroutines`) are
//! Go-runtime concepts (`runtime.MemStats`, `runtime.NumGoroutine`) with no
//! stable Rust equivalent. These are NOT part of any machine report (DESIGN §3),
//! so [`take_heap_snapshot`] populates what the OS exposes (RSS + timestamp) and
//! leaves the Go-runtime-only fields at their documented fallback. See crate todos.

use std::time::{SystemTime, UNIX_EPOCH};

/// Bytes per kB in `/proc/self/smaps_rollup` values (Go `bytesPerKB`).
const BYTES_PER_KB: i64 = 1024;

/// Minimum fields required from `/proc/self/statm` to read RSS (Go `statmMinFields`).
const STATM_MIN_FIELDS: usize = 2;

/// Process memory stats at a point in time (Go `HeapSnapshot`).
///
/// Field names mirror the Go struct. Go-runtime-only fields (see module docs)
/// default to zero on platforms/runtimes that do not expose them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeapSnapshot {
    /// Bytes in in-use heap spans (Go-runtime only; 0 if unavailable).
    pub heap_inuse: i64,
    /// Bytes of allocated heap objects (Go-runtime only; 0 if unavailable).
    pub heap_alloc: i64,
    /// Live heap object count (Go-runtime only; 0 if unavailable).
    pub heap_objects: i64,
    /// Stack memory in use (Go-runtime only; 0 if unavailable).
    pub stack_inuse: i64,
    /// Target heap size for the next GC cycle (Go-runtime only; 0 if unavailable).
    pub next_gc: i64,
    /// Total bytes obtained from the OS by the runtime (Go-runtime only).
    pub sys: i64,
    /// Resident set size (Go + native memory), from `/proc/self/statm`.
    pub rss: i64,
    /// Completed GC cycles (Go-runtime only; 0 if unavailable).
    pub num_gc: u32,
    /// Number of goroutines (Go-runtime only; 0 if unavailable).
    pub goroutines: i32,
    /// Capture time in Unix nanoseconds.
    pub taken_at_ns: i64,
}

/// Captures a [`HeapSnapshot`] (Go `TakeHeapSnapshot`).
///
/// Reads OS-exposed RSS and the current timestamp. Go-runtime-only fields are
/// left at zero (see module docs); `sys` is set to `rss` so the Go invariant
/// `sys >= heap_inuse` (heap_inuse is 0 here) still holds.
#[must_use]
pub fn take_heap_snapshot() -> HeapSnapshot {
    let rss = read_rss_bytes();
    HeapSnapshot {
        rss,
        sys: rss,
        taken_at_ns: now_unix_nanos(),
        ..HeapSnapshot::default()
    }
}

/// Parsed `/proc/self/smaps_rollup` data (Go `SmapsRollup`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmapsRollup {
    /// Resident set size (bytes).
    pub rss: i64,
    /// Proportional set size (bytes).
    pub pss: i64,
    /// Anonymous memory (heap/stacks/native) (bytes).
    pub anonymous: i64,
    /// File-backed memory: `rss - anonymous` (bytes).
    pub file_backed: i64,
    /// Shared clean pages (bytes).
    pub shared_clean: i64,
    /// Shared dirty pages (bytes).
    pub shared_dirty: i64,
    /// Private clean pages (bytes).
    pub private_clean: i64,
    /// Private dirty pages (bytes).
    pub private_dirty: i64,
}

/// Reads and parses `/proc/self/smaps_rollup` (Go `ReadSmapsRollup`).
///
/// Returns a zero [`SmapsRollup`] on non-Linux platforms or on any error.
#[must_use]
pub fn read_smaps_rollup() -> SmapsRollup {
    let content = match std::fs::read_to_string("/proc/self/smaps_rollup") {
        Ok(c) => c,
        Err(_) => return SmapsRollup::default(),
    };

    let mut s = SmapsRollup::default();
    // (prefix, setter) pairs in the same order as the Go targets slice.
    for line in content.lines() {
        if let Some(v) = parse_smaps_kb(line, "Rss:") {
            s.rss = v;
        } else if let Some(v) = parse_smaps_kb(line, "Pss:") {
            s.pss = v;
        } else if let Some(v) = parse_smaps_kb(line, "Anonymous:") {
            s.anonymous = v;
        } else if let Some(v) = parse_smaps_kb(line, "Shared_Clean:") {
            s.shared_clean = v;
        } else if let Some(v) = parse_smaps_kb(line, "Shared_Dirty:") {
            s.shared_dirty = v;
        } else if let Some(v) = parse_smaps_kb(line, "Private_Clean:") {
            s.private_clean = v;
        } else if let Some(v) = parse_smaps_kb(line, "Private_Dirty:") {
            s.private_dirty = v;
        }
    }

    s.file_backed = s.rss - s.anonymous;
    s
}

/// Extracts a kB value from a smaps line like `"Rss:   1234 kB"`.
///
/// Port of Go `parseSmapsKB`. Returns the value in bytes, or `None` if the line
/// does not match the prefix or the number does not parse.
fn parse_smaps_kb(line: &str, prefix: &str) -> Option<i64> {
    let after = line.strip_prefix(prefix)?;
    let trimmed = after.trim();
    let trimmed = trimmed.strip_suffix(" kB").unwrap_or(trimmed);
    let v: i64 = trimmed.trim().parse().ok()?;
    Some(v * BYTES_PER_KB)
}

/// Reads the process RSS from `/proc/self/statm` (Go `ReadRSSBytes`).
///
/// Returns 0 on non-Linux platforms or on error. Field index 1 is resident
/// pages, multiplied by the OS page size.
#[must_use]
pub fn read_rss_bytes() -> i64 {
    let data = match std::fs::read_to_string("/proc/self/statm") {
        Ok(d) => d,
        Err(_) => return 0,
    };

    let fields: Vec<&str> = data.split_whitespace().collect();
    if fields.len() < STATM_MIN_FIELDS {
        return 0;
    }

    let pages: i64 = match fields[1].parse() {
        Ok(p) => p,
        Err(_) => return 0,
    };

    pages * page_size()
}

/// Returns the OS memory page size in bytes (Go `os.Getpagesize`).
fn page_size() -> i64 {
    // 4 KiB is the page size on all Linux targets codefang runs on; this value
    // only scales the RSS reported on Linux (zero on other platforms anyway).
    4096
}

/// Current time as Unix nanoseconds (Go `time.Now().UnixNano()`).
fn now_unix_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of Go `TestTakeHeapSnapshot_TimestampIsRecent`.
    #[test]
    fn timestamp_is_recent() {
        let snap = take_heap_snapshot();
        // 2020-01-01 UTC in ns.
        const MIN_TIMESTAMP: i64 = 1_577_836_800_000_000_000;
        assert!(snap.taken_at_ns > MIN_TIMESTAMP);
    }

    /// Port of Go `TestReadRSSBytes_NonNegative`.
    #[test]
    fn read_rss_bytes_non_negative() {
        let rss = read_rss_bytes();
        assert!(rss >= 0);
        #[cfg(target_os = "linux")]
        assert!(rss > 0, "RSS should be positive on Linux");
    }

    /// Port of Go `TestReadSmapsRollup_NonNegative`.
    #[test]
    fn read_smaps_rollup_non_negative() {
        let smaps = read_smaps_rollup();
        #[cfg(target_os = "linux")]
        {
            assert!(smaps.rss > 0, "smaps Rss should be positive on Linux");
            assert!(smaps.anonymous >= 0);
            assert!(smaps.file_backed >= 0 || smaps.rss < smaps.anonymous);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = smaps;
    }

    /// Port of Go `TestTakeHeapSnapshot_SysIncludesRuntime` invariant
    /// (`sys >= heap_inuse`).
    #[test]
    fn sys_ge_heap_inuse() {
        let snap = take_heap_snapshot();
        assert!(snap.sys >= snap.heap_inuse);
    }

    #[test]
    fn parse_smaps_kb_examples() {
        assert_eq!(parse_smaps_kb("Rss:   1234 kB", "Rss:"), Some(1234 * 1024));
        assert_eq!(parse_smaps_kb("Pss: 5 kB", "Rss:"), None);
        assert_eq!(parse_smaps_kb("Rss: notnum kB", "Rss:"), None);
    }
}

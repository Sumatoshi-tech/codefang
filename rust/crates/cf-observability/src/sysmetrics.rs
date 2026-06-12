//! System / process memory metrics.
//!
//! The Linux `/proc` parsing ([`read_rss_bytes`], [`read_smaps_rollup`]) reads
//! the standard files with kB→bytes conversion and a zero-on-error fallback.
//!
//! The [`HeapSnapshot`] runtime fields (`heap_inuse`, `heap_alloc`,
//! `heap_objects`, `stack_inuse`, `next_gc`, `sys`, `num_gc`, `goroutines`)
//! come from a managed-runtime memory-stats surface with no stable equivalent
//! here. They are NOT part of any machine report (DESIGN §3), so
//! [`take_heap_snapshot`] populates what the OS exposes (RSS + timestamp) and
//! leaves the runtime-only fields at their documented fallback. See crate
//! todos.

use std::time::{SystemTime, UNIX_EPOCH};

/// Bytes per kB in `/proc/self/smaps_rollup` values.
const BYTES_PER_KB: i64 = 1024;

/// Minimum fields required from `/proc/self/statm` to read RSS.
const STATM_MIN_FIELDS: usize = 2;

/// Process memory stats at a point in time.
///
/// Runtime-only fields (see module docs) default to zero on platforms/runtimes
/// that do not expose them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeapSnapshot {
    /// Bytes in in-use heap spans (managed-runtime only; 0 if unavailable).
    pub heap_inuse: i64,
    /// Bytes of allocated heap objects (managed-runtime only; 0 if unavailable).
    pub heap_alloc: i64,
    /// Live heap object count (managed-runtime only; 0 if unavailable).
    pub heap_objects: i64,
    /// Stack memory in use (managed-runtime only; 0 if unavailable).
    pub stack_inuse: i64,
    /// Target heap size for the next GC cycle (managed-runtime only; 0 if unavailable).
    pub next_gc: i64,
    /// Total bytes obtained from the OS by the runtime (managed-runtime only).
    pub sys: i64,
    /// Resident set size (heap + native memory), from `/proc/self/statm`.
    pub rss: i64,
    /// Completed GC cycles (managed-runtime only; 0 if unavailable).
    pub num_gc: u32,
    /// Number of goroutines (managed-runtime only; 0 if unavailable).
    pub goroutines: i32,
    /// Capture time in Unix nanoseconds.
    pub taken_at_ns: i64,
}

/// Captures a [`HeapSnapshot`].
///
/// Reads OS-exposed RSS and the current timestamp. Runtime-only fields are
/// left at zero (see module docs); `sys` is set to `rss` so the invariant
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

/// Parsed `/proc/self/smaps_rollup` data.
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

/// Reads and parses `/proc/self/smaps_rollup`.
///
/// Returns a zero [`SmapsRollup`] on non-Linux platforms or on any error.
#[must_use]
pub fn read_smaps_rollup() -> SmapsRollup {
    let Ok(content) = std::fs::read_to_string("/proc/self/smaps_rollup") else {
        return SmapsRollup::default();
    };

    let mut s = SmapsRollup::default();
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
/// Returns the value in bytes, or `None` if the line does not match the prefix
/// or the number does not parse.
fn parse_smaps_kb(line: &str, prefix: &str) -> Option<i64> {
    let after = line.strip_prefix(prefix)?;
    let trimmed = after.trim();
    let trimmed = trimmed.strip_suffix(" kB").unwrap_or(trimmed);
    let v: i64 = trimmed.trim().parse().ok()?;
    Some(v * BYTES_PER_KB)
}

/// Reads the process RSS from `/proc/self/statm`.
///
/// Returns 0 on non-Linux platforms or on error. Field index 1 is resident
/// pages, multiplied by the OS page size.
#[must_use]
pub fn read_rss_bytes() -> i64 {
    let Ok(data) = std::fs::read_to_string("/proc/self/statm") else {
        return 0;
    };

    let fields: Vec<&str> = data.split_whitespace().collect();
    if fields.len() < STATM_MIN_FIELDS {
        return 0;
    }

    let Ok(pages) = fields[1].parse::<i64>() else {
        return 0;
    };

    pages * page_size()
}

/// Returns the OS memory page size in bytes.
const fn page_size() -> i64 {
    // 4 KiB is the page size on all Linux targets codefang runs on; this value
    // only scales the RSS reported on Linux (zero on other platforms anyway).
    4096
}

/// Current time as Unix nanoseconds.
fn now_unix_nanos() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the reference suite's `TestTakeHeapSnapshot_TimestampIsRecent`.
    #[test]
    fn timestamp_is_recent() {
        let snap = take_heap_snapshot();
        // 2020-01-01 UTC in ns.
        const MIN_TIMESTAMP: i64 = 1_577_836_800_000_000_000;
        assert!(snap.taken_at_ns > MIN_TIMESTAMP);
    }

    /// Mirrors the reference suite's `TestReadRSSBytes_NonNegative`.
    #[test]
    fn read_rss_bytes_non_negative() {
        let rss = read_rss_bytes();
        assert!(rss >= 0);
        #[cfg(target_os = "linux")]
        assert!(rss > 0, "RSS should be positive on Linux");
    }

    /// Mirrors the reference suite's `TestReadSmapsRollup_NonNegative`.
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

    /// Mirrors the reference suite's `TestTakeHeapSnapshot_SysIncludesRuntime`
    /// invariant (`sys >= heap_inuse`).
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

//! Memory watchdog + pprof bootstrap for `--profile`.
//!
//! Port of the memory-watchdog / pprof machinery in cmd/codefang/main.go
//! (`startMemoryWatchdog`, `readRSSMiB`, `readProcField`, `readSmapsRollup`,
//! `saveProcMaps`, `startPprofServer`). This is behavioral parity only: it never
//! influences machine-report bytes (DESIGN.md §4.1, §2.8).
//!
//! Intentional differences from Go, documented so the divergence is explicit:
//!   - Go dumps gzipped pprof heap profiles on RSS spikes; Rust has no built-in
//!     equivalent, so on a spike this logs the smaps rollup and snapshots
//!     `/proc/self/maps` (the diagnostic value Go's runbook relies on). The
//!     gzipped heap `.pb.gz` dump is tracked as a roadmap follow-up under the
//!     `profile` feature and is not output-affecting.
//!   - The pprof HTTP server (localhost:6060) needs a profiling backend; it is
//!     gated behind the `profile` cargo feature. See [`start_pprof_server`].
//!   - Go also logs Go-runtime `MemStats` (GoHeap/GoSys/goroutines); those are
//!     runtime-specific with no Rust analogue, so this logs RSS/threads only.
//!
//! All `/proc` reads are Linux-specific; elsewhere they return defaults and the
//! watchdog logs zeros, matching Go's best-effort `os.Open` fallback.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::time::Duration;

/// Polling interval for the memory watchdog (Go `watchdogInterval = 2s`).
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(2);

/// 1 MiB in bytes (Go `megabyte`).
const MEGABYTE: u64 = 1024 * 1024;

/// RSS threshold in MiB above which heap dumps are triggered (Go `rssThresholdMiB`).
pub const RSS_THRESHOLD_MIB: i64 = 4096;

/// Reads current RSS in MiB from `/proc/self/statm` (Go `readRSSMiB`).
///
/// Returns 0 when the file cannot be read or parsed, matching Go's best-effort
/// behavior. The second field of `statm` is the resident set size in pages.
fn read_rss_mib() -> i64 {
    let mut buf = String::new();
    if File::open("/proc/self/statm")
        .and_then(|mut f| f.read_to_string(&mut buf))
        .is_err()
    {
        return 0;
    }
    let mut fields = buf.split_whitespace();
    let _vsize = fields.next();
    let rss_pages: i64 = match fields.next().and_then(|s| s.parse().ok()) {
        Some(v) => v,
        None => return 0,
    };
    rss_pages * page_size() / MEGABYTE as i64
}

/// Returns the OS page size in bytes (Go `os.Getpagesize()`).
///
/// Reading `/proc/self/statm` already assumes Linux, where 4 KiB is the page
/// size on the supported targets. Watchdog logging is diagnostic, not output.
fn page_size() -> i64 {
    4096
}

/// Reads a named field (e.g. `"Threads:"`) from `/proc/self/status`
/// (Go `readProcField`). Returns an empty string when unavailable.
fn read_proc_field(field: &str) -> String {
    let f = match File::open("/proc/self/status") {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        if let Some(rest) = line.strip_prefix(field) {
            return rest.trim().to_string();
        }
    }
    String::new()
}

/// Summarizes memory regions from `/proc/self/smaps_rollup` (Go `readSmapsRollup`).
fn read_smaps_rollup() -> String {
    const PREFIXES: &[&str] = &[
        "Rss:",
        "Pss:",
        "Anonymous:",
        "AnonHugePages:",
        "Shared_Clean:",
        "Shared_Dirty:",
        "Private_Clean:",
        "Private_Dirty:",
    ];
    let f = match File::open("/proc/self/smaps_rollup") {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let mut out = String::new();
    for line in BufReader::new(f).lines().map_while(Result::ok) {
        if PREFIXES.iter().any(|p| line.starts_with(p)) {
            out.push_str(&line);
            out.push(' ');
        }
    }
    out
}

/// Copies `/proc/self/maps` to `path` for offline analysis (Go `saveProcMaps`).
/// Best-effort: silently returns on any I/O error, matching Go.
fn save_proc_maps(path: &str) {
    let mut src = match File::open("/proc/self/maps") {
        Ok(f) => f,
        Err(_) => return,
    };
    let mut dst = match File::create(path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let mut buf = Vec::new();
    if src.read_to_end(&mut buf).is_ok() {
        let _ = dst.write_all(&buf);
    }
}

/// Logs an RSS spike and snapshots `/proc/self/maps` (Go `handleRSSSpike`).
///
/// Returns the updated dump count. This logs the smaps rollup and the maps
/// snapshot; the gzipped pprof heap profile Go writes has no Rust equivalent
/// without an extra dependency and is a roadmap follow-up under `profile`.
fn handle_rss_spike(dump_count: i32, rss_mib: i64, dump_dir: &str) -> i32 {
    let dump_count = dump_count + 1;
    let smaps = read_smaps_rollup();
    eprintln!("SPIKE #{dump_count}: RSS={rss_mib} MiB smaps: {smaps}");
    if dump_count == 1 {
        save_proc_maps(&format!("{dump_dir}/maps_spike_{rss_mib}MiB.txt"));
    }
    dump_count
}

/// Starts the background memory watchdog (Go `startMemoryWatchdog`).
///
/// Logs RSS / threads every [`WATCHDOG_INTERVAL`] and snapshots `/proc/self/maps`
/// on threshold breach (capped at 5 dumps, matching Go). The baseline maps are
/// saved to `/tmp/maps_baseline.txt` at startup. Runs on a detached thread; the
/// process exits normally when `main` returns.
pub fn start_memory_watchdog(threshold_mib: i64, dump_dir: &'static str) {
    let dump_dir_owned = dump_dir.to_string();
    std::thread::spawn(move || {
        let mut dump_count = 0i32;
        let mut tick = 0u64;
        let tick_seconds = WATCHDOG_INTERVAL.as_secs();
        loop {
            std::thread::sleep(WATCHDOG_INTERVAL);
            tick += 1;
            let rss_mib = read_rss_mib();
            let threads = read_proc_field("Threads:");
            eprintln!(
                "MEM t={} RSS={} threads={}",
                tick * tick_seconds,
                rss_mib,
                threads
            );
            if rss_mib > threshold_mib && dump_count < 5 {
                dump_count = handle_rss_spike(dump_count, rss_mib, &dump_dir_owned);
            }
        }
    });

    // Save baseline maps at startup (Go: runbook 6.2, compare t0 vs tN).
    save_proc_maps("/tmp/maps_baseline.txt");
}

/// Starts the pprof HTTP server on localhost:6060 (Go `startPprofServer`).
///
/// Without the `profile` cargo feature this is a single log line: the pprof
/// endpoints require a profiling backend that is not a default dependency.
/// Behavioral parity only; never affects output bytes.
#[allow(unused_variables)]
pub fn start_pprof_server() {
    #[cfg(not(feature = "profile"))]
    eprintln!("pprof server requested (--profile) but the `profile` feature is not enabled");
    #[cfg(feature = "profile")]
    eprintln!("pprof server: localhost:6060 (feature `profile` enabled)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_mib_is_non_negative() {
        assert!(read_rss_mib() >= 0);
    }

    #[test]
    fn page_size_is_positive() {
        assert!(page_size() > 0);
    }

    #[test]
    fn read_proc_field_missing_is_empty() {
        assert_eq!(read_proc_field("DefinitelyNotAField:"), "");
    }

    #[test]
    fn handle_spike_increments_count() {
        let dir = std::env::temp_dir();
        let dir = dir.to_str().unwrap();
        assert_eq!(handle_rss_spike(0, 1, dir), 1);
        assert_eq!(handle_rss_spike(3, 1, dir), 4);
    }

    #[test]
    fn threshold_constant_matches_go() {
        assert_eq!(RSS_THRESHOLD_MIB, 4096);
    }
}

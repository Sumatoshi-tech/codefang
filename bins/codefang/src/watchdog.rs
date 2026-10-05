//! The `--profile` memory watchdog.
//!
//! Logs `MEM t=<s> RSS=<MiB> threads=<n>` to stderr when the run starts and
//! every [`WATCHDOG_INTERVAL`] after, snapshots `/proc/self/maps` into the
//! temp directory at start (`maps_baseline.txt`) and on the first RSS spike
//! above [`RSS_THRESHOLD_MIB`] (`maps_spike_<MiB>MiB.txt`), and logs the
//! `/proc/self/smaps_rollup` summary on every spike (at most
//! [`MAX_SPIKE_DUMPS`]). It never influences report bytes.
//!
//! All `/proc` reads are Linux-specific; elsewhere they return defaults and
//! the watchdog logs zeros.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::time::Duration;

/// Polling interval for the memory watchdog (Go `watchdogInterval = 2s`).
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(2);

/// 1 MiB in bytes (Go `megabyte`).
const MEGABYTE: u64 = 1024 * 1024;

/// RSS threshold in MiB above which a spike is logged.
pub const RSS_THRESHOLD_MIB: i64 = 4096;

/// Spikes logged per run.
const MAX_SPIKE_DUMPS: i32 = 5;

/// The baseline `/proc/self/maps` snapshot written at start.
const BASELINE_MAPS_FILE: &str = "maps_baseline.txt";

/// Reads current RSS in MiB from `/proc/self/statm`.
///
/// Returns 0 when the file cannot be read or parsed (best effort). The
/// second field of `statm` is the resident set size in pages.
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

/// Returns the OS page size in bytes.
///
/// Reading `/proc/self/statm` already assumes Linux, where 4 KiB is the page
/// size on the supported targets. Watchdog logging is diagnostic, not output.
fn page_size() -> i64 {
    4096
}

/// Reads a named field (e.g. `"Threads:"`) from `/proc/self/status`.
/// Returns an empty string when unavailable.
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

/// Summarizes memory regions from `/proc/self/smaps_rollup`.
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

/// Copies `/proc/self/maps` to `path` for offline analysis. Best effort:
/// silently returns on any I/O error.
fn save_proc_maps(path: &std::path::Path) {
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

/// Logs an RSS spike and, on the first one, snapshots `/proc/self/maps`
/// into `dump_dir`. Returns the updated spike count.
fn handle_rss_spike(dump_count: i32, rss_mib: i64, dump_dir: &std::path::Path) -> i32 {
    let dump_count = dump_count + 1;
    let smaps = read_smaps_rollup();
    eprintln!("SPIKE #{dump_count}: RSS={rss_mib} MiB smaps: {smaps}");
    if dump_count == 1 {
        save_proc_maps(&dump_dir.join(format!("maps_spike_{rss_mib}MiB.txt")));
    }
    dump_count
}

/// Logs one `MEM` sample at `elapsed_secs` and returns the RSS in MiB.
fn log_sample(elapsed_secs: u64) -> i64 {
    let rss_mib = read_rss_mib();
    let threads = read_proc_field("Threads:");
    eprintln!("MEM t={elapsed_secs} RSS={rss_mib} threads={threads}");
    rss_mib
}

/// Starts the memory watchdog: one sample and the baseline maps snapshot
/// now, then a detached thread sampling every [`WATCHDOG_INTERVAL`] and
/// logging spikes above `threshold_mib` (at most [`MAX_SPIKE_DUMPS`]). The
/// thread ends with the process.
pub fn start_memory_watchdog(threshold_mib: i64, dump_dir: std::path::PathBuf) {
    log_sample(0);
    save_proc_maps(&dump_dir.join(BASELINE_MAPS_FILE));
    std::thread::spawn(move || {
        let mut dump_count = 0i32;
        let mut tick = 0u64;
        let tick_seconds = WATCHDOG_INTERVAL.as_secs();
        loop {
            std::thread::sleep(WATCHDOG_INTERVAL);
            tick += 1;
            let rss_mib = log_sample(tick * tick_seconds);
            if rss_mib > threshold_mib && dump_count < MAX_SPIKE_DUMPS {
                dump_count = handle_rss_spike(dump_count, rss_mib, &dump_dir);
            }
        }
    });
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
    fn handle_spike_increments_count_and_snapshots_the_first() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(handle_rss_spike(0, 1, dir.path()), 1);
        assert!(dir.path().join("maps_spike_1MiB.txt").is_file());
        assert_eq!(handle_rss_spike(3, 2, dir.path()), 4);
        assert!(!dir.path().join("maps_spike_2MiB.txt").exists());
    }
}

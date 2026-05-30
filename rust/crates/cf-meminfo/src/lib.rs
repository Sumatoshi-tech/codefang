//! Runtime memory introspection (ported from the Go package `meminfo`).
//!
//! This crate exposes a single helper, [`read_rss_bytes`], that returns the
//! current process resident set size (RSS) in bytes. It is the byte-for-byte
//! behavioral port of `pkg/meminfo` (`ReadRSSBytes`), which is build-tagged
//! per-OS in Go (`rss_linux.go` / `rss_other.go`) and consumed by `analyze`.
//!
//! # Behavior
//!
//! - On Linux, RSS is read from `/proc/self/statm` (the second field, in pages)
//!   and multiplied by the system page size, exactly like the Go implementation
//!   (`rss * int64(os.Getpagesize())`).
//! - On every other platform, and on any error (file missing, unreadable, or
//!   unparsable), the function returns `0` — mirroring Go's "returns 0 if the
//!   information is unavailable" / "returns 0 on non-Linux platforms" contract.
//!
//! The Go function never returns an error; failures collapse to `0`. The Rust
//! port preserves that: the return type is a plain `i64`.
//!
//! # Examples
//!
//! ```
//! let rss = cf_meminfo::read_rss_bytes();
//! assert!(rss >= 0);
//! ```

/// Path to the per-process statm file on Linux.
///
/// Mirrors Go's `procStatmPath` constant in `pkg/meminfo/rss_linux.go`.
#[cfg(target_os = "linux")]
const PROC_STATM_PATH: &str = "/proc/self/statm";

/// Returns the current process RSS (resident set size) in bytes.
///
/// On Linux this reads `/proc/self/statm`, whose second whitespace-separated
/// field is the resident set size in pages, and multiplies it by the system
/// page size (`sysconf(_SC_PAGESIZE)`, the equivalent of Go's
/// `os.Getpagesize()`).
///
/// Returns `0` if the information is unavailable for any reason — the file
/// cannot be opened, cannot be read, or its contents cannot be parsed. This
/// exactly reproduces the Go contract, which silently degrades to `0` rather
/// than surfacing an error.
///
/// # Examples
///
/// ```
/// let rss = cf_meminfo::read_rss_bytes();
/// assert!(rss >= 0);
/// ```
#[cfg(target_os = "linux")]
pub fn read_rss_bytes() -> i64 {
    use std::fs;

    // os.Open + read; any failure -> 0 (Go returns 0 if Open fails).
    let contents = match fs::read_to_string(PROC_STATM_PATH) {
        Ok(contents) => contents,
        Err(_) => return 0,
    };

    // Go scans two integers in sequence: vsize, then rss. fmt.Fscan skips
    // leading whitespace and reads whitespace-separated tokens. We mirror that
    // by splitting on ASCII whitespace and parsing the first two tokens. If
    // either scan fails (missing or non-numeric token), Go returns 0.
    let mut fields = contents.split_ascii_whitespace();

    // First field: vsize. Go reads it but only uses it to advance the scanner.
    if fields.next().and_then(|tok| tok.parse::<i64>().ok()).is_none() {
        return 0;
    }

    // Second field: rss, in pages.
    let rss: i64 = match fields.next().and_then(|tok| tok.parse::<i64>().ok()) {
        Some(rss) => rss,
        None => return 0,
    };

    rss * page_size()
}

/// Returns the current process RSS in bytes.
///
/// On non-Linux platforms this always returns `0`, mirroring Go's
/// `rss_other.go` (`//go:build !linux`).
///
/// # Examples
///
/// ```
/// let rss = cf_meminfo::read_rss_bytes();
/// assert_eq!(rss, 0);
/// ```
#[cfg(not(target_os = "linux"))]
pub fn read_rss_bytes() -> i64 {
    0
}

/// Returns the system memory page size in bytes.
///
/// Equivalent to Go's `os.Getpagesize()`, which on Linux is backed by
/// `sysconf(_SC_PAGESIZE)`. A non-positive or failed lookup falls back to the
/// conventional 4096-byte page so that RSS still scales sensibly.
#[cfg(target_os = "linux")]
fn page_size() -> i64 {
    // SAFETY: sysconf is a thread-safe libc call with no preconditions.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size > 0 {
        size as i64
    } else {
        4096
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from Go's `TestReadRSSBytes_ReturnsNonNegative`: the result must
    /// always be non-negative on every platform.
    #[test]
    fn read_rss_bytes_returns_non_negative() {
        let rss = read_rss_bytes();
        assert!(rss >= 0, "RSS should be non-negative, got {rss}");
    }

    /// Ported from Go's `TestReadRSSBytes_NonZeroOnLinux`: on Linux the live
    /// process always has a positive resident set. On other platforms the Go
    /// test skips; here the function is compiled out and trivially returns 0,
    /// so this assertion is Linux-only.
    #[cfg(target_os = "linux")]
    #[test]
    fn read_rss_bytes_positive_on_linux() {
        let rss = read_rss_bytes();
        assert!(rss > 0, "RSS should be positive on Linux, got {rss}");
    }

    /// On non-Linux platforms the port mirrors `rss_other.go` and returns 0.
    #[cfg(not(target_os = "linux"))]
    #[test]
    fn read_rss_bytes_zero_off_linux() {
        assert_eq!(read_rss_bytes(), 0);
    }

    /// The page size used to scale RSS must be a sane positive value.
    #[cfg(target_os = "linux")]
    #[test]
    fn page_size_is_positive() {
        assert!(page_size() > 0);
    }
}

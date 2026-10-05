//! Runtime memory introspection (ported from the Go package `meminfo`).
//!
//! This crate exposes [`read_rss_bytes`], which returns the current process
//! resident set size (RSS) in bytes, and [`release_free_heap`], which returns
//! freed heap pages to the operating system. It is the byte-for-byte
//! behavioral port of `pkg/meminfo` (`ReadRSSBytes`), which is build-tagged
//! per-OS in Go (`rss_linux.go` / `rss_other.go`) and consumed by `analyze`.
//!
//! # Behavior
//!
//! - On Linux, RSS is read from `/proc/self/statm` (the second field, in pages)
//!   and multiplied by the system page size, exactly like the Go implementation
//!   (`rss * int64(os.Getpagesize())`).
//! - On every other platform, and on any error (file missing, unreadable, or
//!   unparsable), the function returns `0` — the reference contract ("returns 0 if the
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
/// The `/proc/self/statm` path the RSS reader consumes.
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
    if fields
        .next()
        .and_then(|tok| tok.parse::<i64>().ok())
        .is_none()
    {
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

/// Returns the heap's free pages to the operating system.
///
/// glibc keeps freed memory inside its per-thread arenas and gives back only
/// the top of each heap, so a parallel walk whose workers each once held a
/// large transient working set keeps every worker's high-water mark resident
/// long after the memory was freed. Measured on a 7,973-commit kubernetes
/// prefix, `history/sentiment` sat at 1777 MB resident and dropped to 679 MB
/// after a forced trim. This asks glibc to release every whole free page in
/// every arena (`malloc_trim(0)`); live allocations are untouched.
///
/// A no-op on other C libraries and platforms.
///
/// # Examples
///
/// ```
/// cf_meminfo::release_free_heap();
/// ```
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn release_free_heap() {
    // SAFETY: malloc_trim takes no pointers and is thread-safe: it locks each
    // arena while it scans it. Its return value only reports whether anything
    // was released.
    unsafe {
        libc::malloc_trim(0);
    }
}

/// Returns the heap's free pages to the operating system: a no-op off glibc,
/// whose allocators do not expose (or need) an explicit trim.
///
/// # Examples
///
/// ```
/// cf_meminfo::release_free_heap();
/// ```
#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn release_free_heap() {}

/// Returns the system memory page size in bytes.
///
/// Equivalent to `os.Getpagesize()` in the reference runtime, which on Linux is backed by
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

    /// Mirrors reference test `TestReadRSSBytes_ReturnsNonNegative`: the result must
    /// always be non-negative on every platform.
    #[test]
    fn read_rss_bytes_returns_non_negative() {
        let rss = read_rss_bytes();
        assert!(rss >= 0, "RSS should be non-negative, got {rss}");
    }

    /// Mirrors reference test `TestReadRSSBytes_NonZeroOnLinux`: on Linux the live
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

    /// 1 KiB blocks: small enough to live inside a malloc arena, never in a
    /// dedicated mapping.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    const FRAGMENT_BLOCK_SIZE: usize = 1024;
    /// 64 MiB of blocks.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    const FRAGMENT_BLOCKS: usize = 64 * 1024;
    /// One block in this many stays alive, so the freed space around it is
    /// trapped below live data and cannot be returned from the heap's top.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    const PIN_EVERY: usize = 64;
    /// The resident drop that proves trapped free pages went back to the OS
    /// (about 60 MiB of the 64 MiB is free whole pages).
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    const MIN_RELEASED_BYTES: i64 = 32 * 1024 * 1024;

    /// Freed memory trapped between live allocations stays resident until
    /// the heap is asked to release it; afterwards RSS drops by the trapped
    /// pages while the live blocks stay intact.
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    #[test]
    fn release_free_heap_returns_trapped_free_pages() {
        let blocks: Vec<Vec<u8>> = (0..FRAGMENT_BLOCKS)
            .map(|i| vec![(i % 251 + 1) as u8; FRAGMENT_BLOCK_SIZE])
            .collect();
        let pinned: Vec<Vec<u8>> = blocks
            .into_iter()
            .enumerate()
            .filter_map(|(i, block)| (i % PIN_EVERY == 0).then_some(block))
            .collect();
        let before = read_rss_bytes();
        release_free_heap();
        let after = read_rss_bytes();
        assert!(
            pinned
                .iter()
                .enumerate()
                .all(|(i, b)| b.iter().all(|&v| v == ((i * PIN_EVERY) % 251 + 1) as u8)),
            "live blocks survive the release"
        );
        assert!(
            before - after >= MIN_RELEASED_BYTES,
            "released {} bytes (rss {before} -> {after})",
            before - after
        );
    }

    /// Releasing free memory is always safe to call, also with nothing to
    /// release and from several threads at once.
    #[test]
    fn release_free_heap_is_safe_to_call_concurrently() {
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(release_free_heap);
            }
        });
        release_free_heap();
    }

    /// The page size used to scale RSS must be a sane positive value.
    #[cfg(target_os = "linux")]
    #[test]
    fn page_size_is_positive() {
        assert!(page_size() > 0);
    }
}

//! glibc malloc tunables re-exec (Go `ensureMallocTunables`).
//!
//! Port of cmd/codefang/main.go:`ensureMallocTunables`. glibc reads these env
//! vars at the very first `malloc()`, before any threads exist, so setting them
//! from inside the running process is too late — the fix is to set them and
//! `execve` the same binary so the fresh process starts with them in effect.
//!
//! This is memory-behavior parity only and never affects machine-report bytes
//! (DESIGN.md §4.1). It runs before any argument parsing, exactly as in Go
//! (`ensureMallocTunables()` is the first line of `main`).
//!
//! Idempotency guard: if `MALLOC_ARENA_MAX` is already set (the re-exec completed
//! or a user supplied an override), this is a no-op, so the process re-execs at
//! most once.

/// Sets glibc malloc tunables and re-execs self if not already configured.
///
/// On success the `execve` replaces the current process and this never returns.
/// On any failure (cannot resolve the executable, `execve` errors) it logs to
/// stderr and returns so the program continues untuned — matching Go's
/// best-effort behavior.
///
/// On non-Unix targets this is a no-op (the tunables are glibc-specific).
pub fn ensure_malloc_tunables() {
    // Already configured: re-exec completed, or a manual override is present.
    if std::env::var_os("MALLOC_ARENA_MAX").is_some() {
        return;
    }

    #[cfg(unix)]
    unix_reexec();
}

/// The glibc malloc tunables set before re-exec, derived from Go main.go:232-235
/// with ONE deliberate divergence:
///
/// - `MALLOC_ARENA_MAX`: Go sets `2` to cap RSS — sound there because the Go
///   runtime serves nearly all allocations from its own heap and glibc malloc
///   only handles the CGO slice. In this binary EVERYTHING (Rust + the
///   tree-sitter/libgit2 C side) goes through glibc, so 2 arenas serialize the
///   parallel history walks: measured on kubernetes `history/imports --limit
///   10000` at 24 threads, `ARENA_MAX=2` ran 102s (slower than the 82s
///   single-thread walk, with ~46% of CPU inside glibc malloc internals) vs 14s
///   with arenas at glibc's own default formula. RSS stayed ~0.7 GB either way.
///   So we set `8 × cores` — the default glibc would compute if the variable
///   were unset, but it doubles as the re-exec idempotency guard, so it must be
///   set explicitly.
/// - `MALLOC_MMAP_THRESHOLD_=32768`: allocations >= 32 KiB use mmap (returned to
///   the OS on free). Measured neutral for the parallel walks; kept for RSS
///   parity with Go.
/// - `MALLOC_TRIM_THRESHOLD_=16384`: trim arenas aggressively. Neutral; kept.
/// - `MALLOC_MMAP_MAX_=65536`: allow many concurrent mmap regions. Neutral; kept.
fn tunables() -> [(&'static str, String); 4] {
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    [
        ("MALLOC_ARENA_MAX", (8 * cores).to_string()),
        ("MALLOC_MMAP_THRESHOLD_", "32768".to_string()),
        ("MALLOC_TRIM_THRESHOLD_", "16384".to_string()),
        ("MALLOC_MMAP_MAX_", "65536".to_string()),
    ]
}

#[cfg(unix)]
fn unix_reexec() {
    use std::ffi::CString;
    use std::os::raw::c_char;
    use std::os::unix::ffi::OsStrExt;

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return, // best-effort; continue without tuning.
    };

    // Set tunables in the current environment so the child inherits them. (Go
    // mutates the environment via os.Setenv, then passes os.Environ() to Exec.)
    // On edition 2021 `set_var` is safe; it runs before any worker threads, the
    // same single-threaded point as Go's `ensureMallocTunables`.
    for (k, v) in tunables() {
        std::env::set_var(k, v);
    }

    // Build argv (argv[0] + the original arguments) for execv.
    let exe_c = match CString::new(exe.as_os_str().as_bytes()) {
        Ok(c) => c,
        Err(_) => return,
    };
    let argv: Vec<CString> = std::env::args_os()
        .filter_map(|a| CString::new(a.as_bytes()).ok())
        .collect();
    if argv.is_empty() {
        return;
    }
    let mut argv_ptrs: Vec<*const c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    argv_ptrs.push(std::ptr::null());

    // SAFETY: `execv` is called with a NUL-terminated argv built from valid
    // CStrings backed by `argv`, which outlives the call. The tunable env vars
    // were set above and are inherited by the new image. On success this never
    // returns; on failure we fall through and log, matching Go's best effort.
    unsafe {
        execv(exe_c.as_ptr(), argv_ptrs.as_ptr());
    }

    // Reached only if execv failed.
    eprintln!("re-exec failed: errno {}", last_errno());
}

#[cfg(unix)]
extern "C" {
    fn execv(
        path: *const std::os::raw::c_char,
        argv: *const *const std::os::raw::c_char,
    ) -> std::os::raw::c_int;
}

#[cfg(unix)]
fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_op_when_already_configured() {
        // With MALLOC_ARENA_MAX set, ensure_malloc_tunables returns immediately
        // (no re-exec). Force it to be deterministic for the test process.
        std::env::set_var("MALLOC_ARENA_MAX", "2");
        ensure_malloc_tunables(); // must return without re-execing the test binary.
    }

    #[test]
    fn tunables_keep_go_thresholds_and_unthrottle_arenas() {
        // Three tunables keep Go's values (main.go:232-235); MALLOC_ARENA_MAX
        // deliberately diverges to glibc's default formula (8 × cores) because
        // Go's `2` serializes the parallel history walks here (see fn docs).
        let t = tunables();
        let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        assert_eq!(t[0], ("MALLOC_ARENA_MAX", (8 * cores).to_string()));
        assert_eq!(t[1], ("MALLOC_MMAP_THRESHOLD_", "32768".to_string()));
        assert_eq!(t[2], ("MALLOC_TRIM_THRESHOLD_", "16384".to_string()));
        assert_eq!(t[3], ("MALLOC_MMAP_MAX_", "65536".to_string()));
    }
}

//! CPU/heap profiling entry points — port of `internal/framework/profiling.go`.
//!
//! The Go module wraps `runtime/pprof`: `MaybeStartCPUProfile` opens a file and
//! starts CPU profiling (returning a stop closure), and `MaybeWriteHeapProfile`
//! GCs then writes a heap profile. Rust has no equivalent of Go's built-in
//! pprof in the standard library, so the **policy** is ported (the no-op-on-
//! empty-path contract and the create-file-then-record flow) while the actual
//! pprof recording is delegated to an injected backend.
//!
//! This keeps the framework's call sites identical (`let stop =
//! maybe_start_cpu_profile(path, backend)?; ... stop();`) and lets the binary
//! wire in a real profiler (e.g. the `pprof` crate) behind a feature, exactly
//! as `specs/rust-rewrite/DESIGN.md` §4.1 describes profiling as "behavioral
//! parity only" behind an optional feature. The no-profiler default is a
//! faithful no-op.

use std::fs::File;
use std::io;
use std::path::Path;

/// A pluggable CPU/heap profiling backend.
///
/// The default [`NoopProfiler`] does nothing (the framework ships without a
/// profiler by default). A binary can implement this over the `pprof` crate to
/// get behavior matching Go's `runtime/pprof`.
pub trait Profiler {
    /// Begin CPU profiling, writing to `file` when stopped. Returns a token the
    /// caller drops/stops to finish the profile.
    fn start_cpu_profile(&self, file: File) -> io::Result<Box<dyn CpuProfileGuard>>;

    /// Force a GC and write a heap profile to `file`. Mirrors
    /// `runtime.GC(); pprof.WriteHeapProfile(f)`.
    fn write_heap_profile(&self, file: File) -> io::Result<()>;
}

/// RAII guard returned by [`Profiler::start_cpu_profile`]; stopping/dropping it
/// finishes the CPU profile. Mirrors the deferred stop closure Go returns.
pub trait CpuProfileGuard {
    /// Stop the CPU profile and flush. Idempotent.
    fn stop(self: Box<Self>);
}

/// The default no-op profiler: every operation succeeds and records nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopProfiler;

struct NoopGuard;

impl CpuProfileGuard for NoopGuard {
    fn stop(self: Box<Self>) {}
}

impl Profiler for NoopProfiler {
    fn start_cpu_profile(&self, _file: File) -> io::Result<Box<dyn CpuProfileGuard>> {
        Ok(Box::new(NoopGuard))
    }

    fn write_heap_profile(&self, _file: File) -> io::Result<()> {
        Ok(())
    }
}

/// Starts CPU profiling to the given path, returning a stop closure that must be
/// run when profiling should end. A no-op (returning an empty closure) when
/// `path` is empty. Mirrors Go `MaybeStartCPUProfile`.
///
/// # Errors
///
/// Returns an error if the profile file cannot be created or the backend fails
/// to start, mirroring Go's `"could not create CPU profile"` /
/// `"could not start CPU profile"` paths.
pub fn maybe_start_cpu_profile<P: Profiler>(
    path: &str,
    profiler: &P,
) -> io::Result<Box<dyn FnOnce()>> {
    if path.is_empty() {
        return Ok(Box::new(|| {}));
    }

    let file = File::create(Path::new(path)).map_err(|e| {
        io::Error::new(e.kind(), format!("could not create CPU profile: {e}"))
    })?;

    let guard = profiler.start_cpu_profile(file).map_err(|e| {
        io::Error::new(e.kind(), format!("could not start CPU profile: {e}"))
    })?;

    Ok(Box::new(move || guard.stop()))
}

/// Writes a heap profile to the given path. A no-op when `path` is empty.
/// Mirrors Go `MaybeWriteHeapProfile`: errors are logged (here, returned) rather
/// than propagated as fatal. The provided `profiler` does the GC + write.
///
/// Returns the error rather than logging (the binary's caller decides how to
/// surface it; the Go version logs via `slog`).
pub fn maybe_write_heap_profile<P: Profiler>(path: &str, profiler: &P) -> io::Result<()> {
    if path.is_empty() {
        return Ok(());
    }

    let file = File::create(Path::new(path)).map_err(|e| {
        io::Error::new(e.kind(), format!("could not create heap profile: {e}"))
    })?;

    profiler.write_heap_profile(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_profile_empty_path_is_noop() {
        // Empty path -> Ok closure, never touches the filesystem.
        let stop = maybe_start_cpu_profile("", &NoopProfiler).unwrap();
        stop(); // runs cleanly.
    }

    #[test]
    fn heap_profile_empty_path_is_noop() {
        maybe_write_heap_profile("", &NoopProfiler).unwrap();
    }

    #[test]
    fn cpu_profile_writes_file_with_noop_backend() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cf-framework-cpu-{}.prof", std::process::id()));
        let path_str = path.to_str().unwrap();
        let stop = maybe_start_cpu_profile(path_str, &NoopProfiler).unwrap();
        stop();
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn heap_profile_creates_file() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("cf-framework-heap-{}.prof", std::process::id()));
        let path_str = path.to_str().unwrap();
        maybe_write_heap_profile(path_str, &NoopProfiler).unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cpu_profile_bad_path_errors() {
        // A path under a nonexistent directory should fail to create.
        let res = maybe_start_cpu_profile("/nonexistent-dir-xyz/cpu.prof", &NoopProfiler);
        assert!(res.is_err());
        let msg = format!("{}", res.err().unwrap());
        assert!(msg.contains("could not create CPU profile"));
    }
}

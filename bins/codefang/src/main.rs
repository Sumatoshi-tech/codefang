//! `codefang` binary — a thin shell over the `cf-commands` crate.
//!
//! The entire CLI surface (the `run` / `render` / `version` command tree, every
//! literal + dynamic flag) and the analysis dispatch (the general run pipeline +
//! analyzer registry) live in `cf-commands` (DESIGN §1 tier 8). This entrypoint
//! only performs the two pre-parse bootstrap steps that must run in the process
//! before any dispatch — mirroring Go `cmd/codefang/main.go`:
//!
//!  1. [`malloc::ensure_malloc_tunables`] — set glibc malloc env vars and re-exec
//!     self BEFORE anything else (Go `ensureMallocTunables()`, first line of
//!     `main`).
//!  2. `--profile` PersistentPreRun: pprof server + memory watchdog
//!     ([`watchdog`]), behavioral parity only.
//!
//! Then it hands argv to [`cf_commands::run`], which owns parsing + dispatch, and
//! exits with the returned code. The historical per-`(analyzer, format)` dispatch
//! ladder and the duplicated analyzer-orchestration modules have moved into
//! `cf-commands` (`pipeline` + `handlers`); this file no longer contains any.
//!
//! `git2` stays a dependency so the workspace links the vendored libgit2 (DESIGN
//! §3); [`_libgit2_link_anchor`] keeps a core libgit2 symbol referenced.

mod malloc;
mod watchdog;

use std::process::exit;

fn main() {
    // (1) glibc malloc tunables BEFORE any parsing; re-execs self on first run
    // (Go `ensureMallocTunables()`, the first line of main).
    malloc::ensure_malloc_tunables();

    // (2) --profile PersistentPreRun: pprof server + memory watchdog. We scan
    // argv directly (the flag is global/persistent) so the watchdog starts
    // before cf-commands parses and dispatches, matching cobra's PreRun order.
    if std::env::args().any(|a| a == "--profile") {
        watchdog::start_pprof_server();
        watchdog::start_memory_watchdog(watchdog::RSS_THRESHOLD_MIB, "/tmp");
    }

    // (3) Parse + dispatch run / render / version through the general pipeline.
    exit(cf_commands::run(std::env::args_os()));
}

/// Keeps `git2` (and thus the vendored libgit2) in this binary's dependency
/// graph. `Oid::from_bytes` links a core libgit2 path. DESIGN §3 keeps libgit2
/// for byte-identical diff/blob/hash semantics.
#[allow(dead_code)]
fn _libgit2_link_anchor() -> bool {
    git2::Oid::from_bytes(&[0u8; 20]).is_ok()
}

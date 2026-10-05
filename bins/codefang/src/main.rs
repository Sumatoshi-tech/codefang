//! `codefang` binary — a thin shell over the `cf-commands` crate.
//!
//! The entire CLI surface (the `run` / `version` / `completion` command tree,
//! every literal + dynamic flag, the configuration file) and the analysis
//! dispatch live in `cf-commands`. This entrypoint only performs the two
//! steps that must run before any parsing:
//!
//!  1. [`malloc::ensure_malloc_tunables`] — set glibc malloc env vars and
//!     re-exec self BEFORE anything else.
//!  2. `--profile` — start the memory watchdog ([`watchdog`]).
//!
//! Then it hands argv to [`cf_commands::run`], which owns parsing + dispatch,
//! and exits with the returned code.
//!
//! `git2` stays a dependency so the workspace links the vendored libgit2;
//! [`_libgit2_link_anchor`] keeps a core libgit2 symbol referenced.

mod malloc;
mod watchdog;

use std::process::exit;

fn main() {
    // (1) glibc malloc tunables BEFORE any parsing; re-execs self on first run.
    malloc::ensure_malloc_tunables();

    // (2) --profile: the memory watchdog. argv is scanned directly (the flag
    // is global) so the watchdog samples the whole run, parsing included.
    if std::env::args().any(|a| a == "--profile") {
        watchdog::start_memory_watchdog(watchdog::RSS_THRESHOLD_MIB, std::env::temp_dir());
    }

    // (3) Parse + dispatch run / version / completion.
    exit(cf_commands::run(std::env::args_os()));
}

/// Keeps `git2` (and thus the vendored libgit2) in this binary's dependency
/// graph. `Oid::from_bytes` links a core libgit2 path. DESIGN §3 keeps libgit2
/// for byte-identical diff/blob/hash semantics.
#[allow(dead_code)]
fn _libgit2_link_anchor() -> bool {
    git2::Oid::from_bytes(&[0u8; 20]).is_ok()
}

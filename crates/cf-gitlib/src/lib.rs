//! `cf-gitlib` — git2-backed repository/commit/diff/blob access layer.
//!
//! Wires the per-concern modules (repository, revwalk, commit, tree, blob,
//! diff, changes, hash, signature, file, helpers, worker, batch) into one crate
//! and re-exports the handful of types the rest of the workspace refers to as
//! `cf_gitlib::Repository` / `Commit` / `Blob` / `Signature` / `GitError`.
//!
//! Repository handles are per-thread (`!Send`/`!Sync`) and rely on RAII `Drop`
//! to free libgit2 objects. Everything that surfaces into machine reports
//! (hash rendering, diff line counts, tree-change streams) reproduces the
//! reference implementation exactly; output bytes are pinned against the
//! reference binary by `tests/compat`.

// Compile the README's usage example as a doctest (without injecting the README
// into the rendered crate docs, which the `//!` block above already covers).
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

pub mod batch;
pub mod blob;
pub mod changes;
pub mod commit;
pub mod diff;
pub mod error;
pub mod file;
pub mod hash;
pub mod helpers;
pub mod repository;
pub mod revwalk;
pub mod signature;
pub mod tree;
pub mod worker;

#[cfg(test)]
pub mod testing;
#[cfg(test)]
pub mod testutil;

pub use blob::Blob;
pub use commit::{Commit, CommitIter};
pub use error::{GitCause, GitError, Result};
pub use hash::Hash;
pub use repository::{LogOptions, Repository};
pub use revwalk::RevWalk;
pub use signature::Signature;

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-gitlib";

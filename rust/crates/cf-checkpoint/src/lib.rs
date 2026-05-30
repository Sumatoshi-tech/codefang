//! `cf-checkpoint` — incremental-analysis checkpoint/restore for crash recovery.
//!
//! Port of the Go `internal/checkpoint` package. Used by the analysis framework
//! and by the burndown / couples analyzers to snapshot streaming progress so a
//! long-running analysis can resume after a crash or interruption instead of
//! re-scanning the whole repository.
//!
//! # Model
//!
//! A [`Manager`] owns one checkpoint directory per repository, keyed by a short
//! [`repo_hash`] of the repo path:
//!
//! ```text
//! <base_dir>/<repo_hash>/
//!   checkpoint.json     # Metadata: version, repo, analyzer set, StreamingState
//!   analyzer_0/         # opaque per-analyzer state (one dir per Checkpointable)
//!   analyzer_1/
//! ```
//!
//! Analyzers that can be snapshotted implement [`Checkpointable`]. The manager
//! coordinates them: [`Manager::save`] writes each analyzer's state plus the
//! metadata atomically; [`Manager::validate`] confirms a stored checkpoint
//! matches the current repo/analyzer set/format version; [`Manager::load`]
//! restores analyzer state and returns the saved [`StreamingState`].
//!
//! # Serialization (DESIGN §3)
//!
//! * Checkpoint **metadata** is JSON, written Go-`encoding/json`-byte-compatibly
//!   (HTML escaping on, two-space indent, one trailing newline) via
//!   [`JsonCodec`] so `checkpoint.json` diffs cleanly against the Go build.
//! * Per-analyzer binary state uses [`GobCodec`] — a Rust-native `bincode` codec
//!   standing in for Go's `encoding/gob` (the gob wire format is Go-specific and
//!   never user-visible, so it is intentionally not reproduced).
//!
//! # Example
//!
//! ```no_run
//! use cf_checkpoint::{Manager, StreamingState, repo_hash};
//!
//! let repo = "/path/to/repo";
//! let mgr = Manager::new("/tmp/checkpoints", repo_hash(repo));
//! let state = StreamingState { total_commits: 100, processed_commits: 40, ..Default::default() };
//!
//! // Save (no checkpointable analyzers in this minimal example).
//! mgr.save_now(&mut [], state, repo, &["burndown".to_string()]).unwrap();
//!
//! // Later, after a restart:
//! if mgr.exists() && mgr.validate(repo, &["burndown".to_string()]).is_ok() {
//!     let resumed = mgr.load(&mut []).unwrap();
//!     println!("resume from commit {}", resumed.processed_commits);
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod checkpointable;
mod codec;
mod error;
mod manager;
mod state;

pub use checkpointable::Checkpointable;
pub use codec::{load_state, save_state, Codec, GobCodec, JsonCodec, Persister};
pub use error::{CheckpointError, Result};
pub use manager::{
    default_dir, repo_hash, Manager, DEFAULT_MAX_AGE, DEFAULT_MAX_SIZE, METADATA_VERSION,
};
pub use state::{AggregatorSpillEntry, Metadata, StreamingState};

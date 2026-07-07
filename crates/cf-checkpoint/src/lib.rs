//! `cf-checkpoint` — incremental-analysis checkpoint/restore for crash recovery.
//!
//! Used by the analysis framework and by the burndown / couples analyzers to
//! snapshot streaming progress so a long-running analysis can resume after a
//! crash or interruption instead of re-scanning the whole repository.
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
//! * Checkpoint **metadata** is JSON in the pinned report-format byte layout
//!   (HTML escaping on, two-space indent, one trailing newline) via
//!   [`JsonCodec`], so `checkpoint.json` diffs cleanly against the reference
//!   binary's output.
//! * Per-analyzer binary state uses [`GobCodec`] — a Rust-native `bincode`
//!   codec (this state is internal-only and never user-visible, so no
//!   cross-implementation wire format is reproduced).
//!
//! # Example
//!
//! The checkpoint base directory and the `repo_path` are independent: the repo
//! path is only hashed and recorded, never opened, so this example runs fully
//! against a temporary directory.
//!
//! ```
//! use cf_checkpoint::{Manager, StreamingState, repo_hash};
//!
//! let base = tempfile::tempdir().unwrap();
//! let repo = "/path/to/repo";
//! let mgr = Manager::new(base.path(), repo_hash(repo));
//! let state = StreamingState { total_commits: 100, processed_commits: 40, ..Default::default() };
//!
//! // No checkpoint exists yet.
//! assert!(!mgr.exists());
//!
//! // Save (no checkpointable analyzers in this minimal example).
//! mgr.save_now(&mut [], state, repo, &["burndown".to_string()]).unwrap();
//! assert!(mgr.exists());
//!
//! // Later, after a restart: validate matches the same repo/analyzer set,
//! // then resume from the saved progress.
//! assert!(mgr.validate(repo, &["burndown".to_string()]).is_ok());
//! let resumed = mgr.load(&mut []).unwrap();
//! assert_eq!(resumed.processed_commits, 40);
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

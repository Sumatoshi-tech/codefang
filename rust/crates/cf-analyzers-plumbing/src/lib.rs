//! cf-analyzers-plumbing: analyzer-level git/UAST data providers.
//!
//! These providers sit beneath the history analyzers and supply the shared
//! git/UAST facts the rest of the pipeline consumes:
//!
//! | Provider | Output |
//! |----------|--------|
//! | [`tree_diff::TreeDiff`] | `changes` |
//! | [`blob_cache::BlobCache`] | `blob_cache` |
//! | [`file_diff::FileDiff`] | `file_diff` |
//! | [`ticks::TicksSinceStart`] | `tick` |
//! | [`identity_detector::IdentityDetector`] | `author` |
//! | [`languages_detection::LanguagesDetection`] | `languages` |
//! | [`uast_changes::UASTChanges`] | `uast_changes` |
//!
//! # Behavioral fidelity
//!
//! These providers emit intermediate facts consumed by downstream analyzers,
//! which own the machine-format serialization whose bytes are pinned against
//! the reference implementation by `rust/tests/compat`. The plumbing layer's
//! job is to compute those facts identically:
//!
//! * [`ticks::TicksSinceStart`] implements the floored tick origin, the
//!   committer-time sanitization window ([`ticks_anomaly`]), and the monotonic
//!   `max(tick, previous_tick)` clamp — a byte-identity hazard. Its only
//!   wall-clock read (now + max clock skew) goes through an injectable
//!   [`clock::Clock`] per DESIGN §2.8.
//! * [`file_diff::FileDiff`] implements the Modify-only scope, the two fast
//!   paths, the binary guard, and the encoded-line LOC counts; the actual
//!   line diff is delegated to an injected [`file_diff::LineDiffer`] (the
//!   byte-faithful engine is `cf-godiff`).
//! * [`languages_detection::LanguagesDetection`] carries the frozen extension
//!   fast-path table and delegates content analysis to an injected
//!   [`languages_detection::EnryClassifier`] that must carry the enry data
//!   tables (DESIGN §2.6).
//! * [`blob_cache::BlobCache`] implements the cross-commit previous-cache
//!   reuse and empty-placeholder-on-read-failure semantics.
//!
//! # Dependency boundaries
//!
//! The [`analyzer::Analyzer`] trait, the git model ([`git_model`]), the UAST
//! [`Parser`]/[`PathFilter`] surface ([`uast_iface`]), and the diff/enry
//! engines are defined locally and injected, keeping this crate decoupled
//! from the heavier framework/UAST/git crates. They should collapse into
//! re-exports of the canonical crates as those stabilize.
//!
//! # Example
//!
//! Two of the pure, self-contained helpers this crate exposes: the frozen
//! extension-to-language fast path and the injectable clock seam used to keep
//! tick computation deterministic.
//!
//! ```
//! use cf_analyzers_plumbing::{language_by_extension, Clock, FixedClock};
//!
//! // Extension lookup is case-insensitive and returns "" on no match.
//! assert_eq!(language_by_extension("main.rs"), "Rust");
//! assert_eq!(language_by_extension("App.PY"), "Python");
//! assert_eq!(language_by_extension("README"), "");
//!
//! // A FixedClock yields a deterministic "now" for reproducible goldens.
//! let clock = FixedClock(1_700_000_000);
//! assert_eq!(clock.now_unix(), 1_700_000_000);
//! ```

pub mod analyzer;
pub mod blob_cache;
pub mod clock;
pub mod file_diff;
pub mod git_model;
pub mod identity_detector;
pub mod languages_detection;
pub mod ticks;
pub mod ticks_anomaly;
pub mod tree_diff;
pub mod uast_changes;
pub mod uast_iface;

// Convenient re-exports of the provider types and the values they thread
// through the pipeline.
pub use analyzer::{Analyzer, AnalyzerError, AnyValue, Facts, ValueMap};
pub use blob_cache::{BlobCache, BlobSource, CachedBlob, ErrorBinary, GitBlobSource};
pub use clock::{Clock, FixedClock, SystemClock};
pub use file_diff::{
    Diff, DiffOp, DiffParams, FileDiff, FileDiffData, LineDiffResult, LineDiffer,
    DEFAULT_DIFF_TIMEOUT_MS,
};
pub use git_model::{Action, Change, ChangeEntry, Changes, Commit, Hash, Signature};
pub use identity_detector::{IdentityDetector, AUTHOR_MISSING, AUTHOR_MISSING_NAME};
pub use languages_detection::{language_by_extension, EnryClassifier, LanguagesDetection};
pub use ticks::{TicksSinceStart, DEFAULT_TICKS_SINCE_START_TICK_SIZE_HOURS};
pub use ticks_anomaly::{
    TimeAnomalyStats, TimeAnomalyTracker, MAX_CLOCK_SKEW_SECS, MIN_SANE_COMMIT_TIME_UNIX,
};
pub use tree_diff::{GitTreeSource, TreeDiff, TreeSource};
pub use uast_changes::{UASTChange, UASTChanges, MAX_UAST_BLOB_SIZE};
pub use uast_iface::{
    AllowAllPathFilter, Node, NodeLike, Parser, PathFilter, SharedParser, SharedPathFilter,
};

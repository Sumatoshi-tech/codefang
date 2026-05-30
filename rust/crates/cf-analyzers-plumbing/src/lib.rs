//! cf-analyzers-plumbing: analyzer-level git/UAST data providers.
//!
//! Port of the Go package `internal/analyzers/plumbing`. These providers sit
//! beneath the history analyzers and supply the shared git/UAST facts the rest
//! of the pipeline consumes:
//!
//! | Provider | Output | Go source |
//! |----------|--------|-----------|
//! | [`TreeDiff`](tree_diff::TreeDiff) | `changes` | `tree_diff.go` |
//! | [`BlobCache`](blob_cache::BlobCache) | `blob_cache` | `blob_cache.go` |
//! | [`FileDiff`](file_diff::FileDiff) | `file_diff` | `file_diff.go` |
//! | [`TicksSinceStart`](ticks::TicksSinceStart) | `tick` | `ticks.go` + `ticks_anomaly.go` |
//! | [`IdentityDetector`](identity_detector::IdentityDetector) | `author` | `identity.go` |
//! | [`LanguagesDetection`](languages_detection::LanguagesDetection) | `languages` | `languages.go` |
//! | [`UASTChanges`](uast_changes::UASTChanges) | `uast_changes` | `uast.go` |
//!
//! # Behavioral fidelity
//!
//! These providers emit intermediate facts consumed by downstream analyzers,
//! which own the machine-format serialization that must be byte-identical to Go.
//! The plumbing layer's job is to compute those facts identically:
//!
//! * [`TicksSinceStart`](ticks::TicksSinceStart) reproduces the
//!   `FloorTime`-seeded origin, committer-time sanitization window
//!   ([`ticks_anomaly`]), and the monotonic `max(tick, previousTick)` clamp —
//!   the byte-identity hazard called out in the brief. Its only wall-clock read
//!   (`time.Now().Add(maxClockSkew)`) goes through an injectable [`clock::Clock`]
//!   per DESIGN §2.8.
//! * [`FileDiff`](file_diff::FileDiff) reproduces the Modify-only scope, the two
//!   fast paths, the binary guard, and the `len(src)`/`len(dst)` LOC counts; the
//!   actual line diff is delegated to an injected
//!   [`LineDiffer`](file_diff::LineDiffer) (a faithful `sergi/go-diff` port).
//! * [`LanguagesDetection`](languages_detection::LanguagesDetection) ports the
//!   extension fast-path table verbatim and delegates content analysis to an
//!   injected [`EnryClassifier`](languages_detection::EnryClassifier) that must
//!   carry go-enry's data tables (DESIGN §2.6).
//! * [`BlobCache`](blob_cache::BlobCache) reproduces the cross-commit
//!   previous-cache reuse and empty-placeholder-on-read-failure semantics.
//!
//! # Dependency boundaries (port rule 5)
//!
//! `cf-framework`, `cf-uast`, `cf-gitlib`, `cf-pathfilter`, and the go-compat
//! serialization crates were stubs at port time, so the
//! [`Analyzer`](analyzer::Analyzer) trait, the git model ([`git_model`]), the
//! UAST [`Parser`]/[`PathFilter`] surface ([`uast_iface`]), and the diff/enry
//! engines are defined locally and injected. They should collapse into
//! re-exports of the canonical crates once those exist; see the crate `todos`.

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

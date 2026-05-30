//! `cf-plumbing` — core git plumbing shared types, keys, and fact accessors.
//!
//! Port of the Go package `internal/plumbing`
//! (`github.com/Sumatoshi-tech/codefang/internal/plumbing`), whose stated
//! purpose is "Core git plumbing for analysis (tree diffs, blob access)
//! bridging gitlib to identity. Used by framework, analyze, most analyzers."
//!
//! The Go package itself contains **no serialization output paths** — it only
//! defines:
//!
//! * dependency / fact **key constants** (`keys.go`),
//! * shared **types** re-exported or bridged from `gitlib` (`types.go`),
//! * typed **fact accessors** over the dynamic `facts map[string]any`
//!   (`fact_accessors.go`).
//!
//! Because nothing in this module emits a MACHINE-format report, there is no
//! `cf-gojson` / `cf-goyaml` routing to do here. The optional `serde` derives on
//! [`LineStats`] mirror the Go `json:"…" yaml:"…"` struct tags purely so that
//! *downstream* crates (which DO route through the shared go-compat
//! serialization crate) observe the exact same field names and declaration
//! order; the field order below is the byte-identity-relevant invariant.
//!
//! # Bridged identity constants
//!
//! In Go this package references three `identity.*` fact-key constants. Two of
//! them — the reversed people dictionary and the people count — are owned by the
//! `cf-identity` crate and re-exported here verbatim
//! ([`FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT`],
//! [`FACT_IDENTITY_DETECTOR_PEOPLE_COUNT`]) so there is a single source of truth.
//!
//! # Bridged git types
//!
//! In Go this package aliases [`CachedBlob`], re-exports [`ErrBinary`], and uses
//! `gitlib.Hash` as the element type of the commits-by-tick fact map. Those
//! definitions are owned by `pkg/gitlib` (the `cf-gitlib` crate). While
//! `cf-gitlib` does not yet publish those types through its crate root, this
//! crate defines a minimal, behavior-faithful bridge surface locally. Once
//! `cf-gitlib` exports `Hash`, `CachedBlob`, and the binary-blob error, the
//! definitions below collapse into plain `pub use cf_gitlib::…;` re-exports with
//! no change to this crate's public API (the names and shapes already match).

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::time::Duration;

// Re-export the two identity fact keys from their owning crate so Go's
// `identity.FactIdentityDetector{ReversedPeopleDict,PeopleCount}` references map
// to a single definition shared across the whole rewrite.
pub use cf_identity::{
    FACT_IDENTITY_DETECTOR_PEOPLE_COUNT, FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT,
};

// ---------------------------------------------------------------------------
// keys.go — dependency and fact key constants.
// ---------------------------------------------------------------------------

/// Name of the dependency provided by `FileDiff`.
///
/// Go: `plumbing.DependencyFileDiff`.
pub const DEPENDENCY_FILE_DIFF: &str = "file_diff";

/// Name of the dependency provided by `TreeDiff`.
///
/// Go: `plumbing.DependencyTreeChanges`.
pub const DEPENDENCY_TREE_CHANGES: &str = "changes";

/// Name of the dependency which `DaysSinceStart` provides — the number of ticks
/// since the first commit in the analyzed sequence.
///
/// Go: `plumbing.DependencyTick`.
pub const DEPENDENCY_TICK: &str = "tick";

/// Mapping between day indices and the corresponding commits.
///
/// Go: `plumbing.FactCommitsByTick`.
pub const FACT_COMMITS_BY_TICK: &str = "TicksSinceStart.Commits";

/// The [`Duration`] of each tick.
///
/// Go: `plumbing.FactTickSize`.
pub const FACT_TICK_SIZE: &str = "TicksSinceStart.TickSize";

/// Identifies the dependency provided by `BlobCache`.
///
/// Go: `plumbing.DependencyBlobCache`.
pub const DEPENDENCY_BLOB_CACHE: &str = "blob_cache";

/// Name of the dependency provided by `LanguagesDetection`.
///
/// Go: `plumbing.DependencyLanguages`.
pub const DEPENDENCY_LANGUAGES: &str = "languages";

/// Identifier of the data provided by `LinesStatsCalculator` — line statistics
/// for each file in the commit.
///
/// Go: `plumbing.DependencyLineStats`.
pub const DEPENDENCY_LINE_STATS: &str = "line_stats";

// ---------------------------------------------------------------------------
// types.go — shared types.
// ---------------------------------------------------------------------------

/// Git object hash.
///
/// Bridges `gitlib.Hash` (git SHA-1, a fixed 20-byte array — the same layout
/// `git2`'s `Oid` uses). Used here only as the element type of the
/// commits-by-tick fact map. The byte layout (`[u8; 20]`) matches `cf-gitlib`'s
/// `Hash(pub [u8; 20])` newtype exactly, so this alias is a drop-in for the
/// `cf-gitlib` type once that crate publishes it through its crate root.
pub type Hash = [u8; 20];

/// Raised in [`CachedBlob::count_lines`] when the file is binary.
///
/// Bridges `plumbing.ErrBinary` (= `gitlib.ErrBinary` = `errors.New("binary")`).
/// Its [`std::fmt::Display`] renders exactly `"binary"`, matching the Go
/// sentinel and `cf-gitlib`'s `GitError::Binary` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrBinary;

impl std::fmt::Display for ErrBinary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("binary")
    }
}

impl std::error::Error for ErrBinary {}

/// A single diff edit operation, mirroring `diffmatchpatch.Diff`
/// (`github.com/sergi/go-diff/diffmatchpatch`).
///
/// `FileDiffData.Diffs` is `[]diffmatchpatch.Diff`; the Go type is a pair of
/// `{Type Operation; Text string}`. This port keeps the same shape so the
/// `FileDiff` analyzer can be ported faithfully on top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    /// The edit operation kind.
    pub operation: DiffOperation,
    /// The associated text span.
    pub text: String,
}

/// Diff edit operation kind, mirroring `diffmatchpatch.Operation`
/// (`DiffDelete = -1`, `DiffEqual = 0`, `DiffInsert = 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum DiffOperation {
    /// Text was removed (`diffmatchpatch.DiffDelete`).
    Delete = -1,
    /// Text is unchanged (`diffmatchpatch.DiffEqual`).
    Equal = 0,
    /// Text was added (`diffmatchpatch.DiffInsert`).
    Insert = 1,
}

/// The type of the dependency provided by `FileDiff`.
///
/// Go: `plumbing.FileDiffData`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDiffData {
    /// The list of diff edit operations.
    pub diffs: Vec<Diff>,
    /// Lines of code in the old version of the file.
    pub old_lines_of_code: i32,
    /// Lines of code in the new version of the file.
    pub new_lines_of_code: i32,
}

/// Bridged stand-in for `gitlib.CachedBlob` (aliased in Go as
/// `plumbing.CachedBlob`).
///
/// Models the documented surface of the Go alias — owned blob bytes plus a
/// `CountLines()` that returns [`ErrBinary`] for binary content. The owning
/// implementation (with `git2` loading, the memoized line count, and the full
/// `cf-textutil` binary-detection) lives in `cf-gitlib::CachedBlob`; this bridge
/// reproduces the same observable contract so analyzers that only need
/// `count_lines` can be ported against `cf-plumbing` directly, and it is shape-
/// compatible with a future `pub use cf_gitlib::CachedBlob;` re-export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CachedBlob {
    /// Raw blob contents (`git2`: `blob.content().to_vec()`).
    pub data: Vec<u8>,
}

impl CachedBlob {
    /// Constructs a cached blob from raw bytes (Go `NewCachedBlobForTest`).
    #[must_use]
    pub fn from_data(data: Vec<u8>) -> Self {
        CachedBlob { data }
    }

    /// Counts the number of lines in the blob, returning [`ErrBinary`] if the
    /// content is detected as binary.
    ///
    /// Mirrors the `gitlib.CachedBlob.CountLines` contract: binary blobs (those
    /// containing a NUL byte) yield [`ErrBinary`]; otherwise the count is the
    /// number of `\n`-terminated lines plus a trailing partial line, and an
    /// empty blob counts as zero. The authoritative binary heuristic and line
    /// counter ship with `cf-textutil` (used by `cf-gitlib`); this bridge keeps
    /// the same observable result for the common cases analyzers rely on.
    ///
    /// # Errors
    ///
    /// Returns [`ErrBinary`] when the blob contains a NUL byte.
    pub fn count_lines(&self) -> Result<usize, ErrBinary> {
        if self.data.contains(&0) {
            return Err(ErrBinary);
        }
        if self.data.is_empty() {
            return Ok(0);
        }
        let newlines = self.data.iter().filter(|&&b| b == b'\n').count();
        let trailing = usize::from(self.data.last() != Some(&b'\n'));
        Ok(newlines + trailing)
    }
}

/// Holds the numbers of inserted, deleted and changed lines.
///
/// Go: `plumbing.LineStats`. Field declaration order — `added`, `removed`,
/// `changed` — is byte-identity-relevant for any downstream wrapper that
/// serializes this struct in source order; the `serde` rename attributes mirror
/// the Go `json`/`yaml` tags exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LineStats {
    /// Number of added lines by a particular developer in a particular day.
    #[cfg_attr(feature = "serde", serde(rename = "added"))]
    pub added: i32,
    /// Number of removed lines by a particular developer in a particular day.
    #[cfg_attr(feature = "serde", serde(rename = "removed"))]
    pub removed: i32,
    /// Number of changed lines by a particular developer in a particular day.
    #[cfg_attr(feature = "serde", serde(rename = "changed"))]
    pub changed: i32,
}

// ---------------------------------------------------------------------------
// fact_accessors.go — typed accessors over the dynamic facts map.
//
// In Go, `facts` is `map[string]any` and each accessor performs a type
// assertion: `val, ok := facts[key].(T)`. The faithful Rust equivalent models
// the heterogeneous fact map as `HashMap<String, FactValue>` and each accessor
// returns `Option<T>` (where `Some`/`None` mirrors Go's `ok` boolean and a type
// mismatch yields `None`, exactly like a failed type assertion).
// ---------------------------------------------------------------------------

/// A value stored in the analyzer facts map.
///
/// Models the heterogeneous `map[string]any` used by the Go pipeline. Only the
/// variants the `plumbing` accessors read are enumerated; downstream crates may
/// extend this enum as more fact types are ported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactValue {
    /// A [`Duration`] (e.g. [`FACT_TICK_SIZE`]).
    Duration(Duration),
    /// A commits-by-tick mapping (e.g. [`FACT_COMMITS_BY_TICK`]).
    CommitsByTick(HashMap<i32, Vec<Hash>>),
    /// A list of strings (e.g. the reversed people dictionary).
    StringList(Vec<String>),
    /// A signed integer count (e.g. the people count).
    Int(i64),
    /// A string scalar (exercises the type-mismatch paths).
    Str(String),
}

/// The analyzer facts map: `map[string]any` in Go.
pub type Facts = HashMap<String, FactValue>;

/// Extracts the tick duration from the facts map.
///
/// Go: `plumbing.GetTickSize`. Returns [`None`] when the key is absent or holds
/// the wrong type (mirroring the failed `.(time.Duration)` assertion).
#[must_use]
pub fn get_tick_size(facts: &Facts) -> Option<Duration> {
    match facts.get(FACT_TICK_SIZE) {
        Some(FactValue::Duration(d)) => Some(*d),
        _ => None,
    }
}

/// Extracts the commits-by-tick mapping from the facts map.
///
/// Go: `plumbing.GetCommitsByTick`. The key type is `int` in Go; modeled here as
/// `i32` (tick indices are small). Returns [`None`] on absence or type mismatch.
#[must_use]
pub fn get_commits_by_tick(facts: &Facts) -> Option<&HashMap<i32, Vec<Hash>>> {
    match facts.get(FACT_COMMITS_BY_TICK) {
        Some(FactValue::CommitsByTick(m)) => Some(m),
        _ => None,
    }
}

/// Extracts the reversed people dictionary from the facts map.
///
/// Go: `plumbing.GetReversedPeopleDict`. Reads
/// [`FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT`]. Returns [`None`] on absence
/// or type mismatch.
#[must_use]
pub fn get_reversed_people_dict(facts: &Facts) -> Option<&[String]> {
    match facts.get(FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT) {
        Some(FactValue::StringList(v)) => Some(v.as_slice()),
        _ => None,
    }
}

/// Extracts the unique author count from the facts map.
///
/// Go: `plumbing.GetPeopleCount`. Reads [`FACT_IDENTITY_DETECTOR_PEOPLE_COUNT`].
/// Returns [`None`] on absence or type mismatch.
#[must_use]
pub fn get_people_count(facts: &Facts) -> Option<i64> {
    match facts.get(FACT_IDENTITY_DETECTOR_PEOPLE_COUNT) {
        Some(FactValue::Int(n)) => Some(*n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from fact_accessors_test.go::TestGetTickSize.
    #[test]
    fn test_get_tick_size() {
        // present_with_correct_type
        let mut facts = Facts::new();
        facts.insert(
            FACT_TICK_SIZE.to_string(),
            FactValue::Duration(Duration::from_secs(24 * 60 * 60)),
        );
        assert_eq!(get_tick_size(&facts), Some(Duration::from_secs(24 * 60 * 60)));

        // absent
        let facts = Facts::new();
        assert_eq!(get_tick_size(&facts), None);

        // wrong_type
        let mut facts = Facts::new();
        facts.insert(
            FACT_TICK_SIZE.to_string(),
            FactValue::Str("not a duration".to_string()),
        );
        assert_eq!(get_tick_size(&facts), None);
    }

    // Ported from fact_accessors_test.go::TestGetCommitsByTick.
    #[test]
    fn test_get_commits_by_tick() {
        let sample_hash: Hash = {
            let mut h = [0u8; 20];
            h[0] = 0x01;
            h
        };
        let mut sample_map: HashMap<i32, Vec<Hash>> = HashMap::new();
        sample_map.insert(0, vec![sample_hash]);

        // present_with_correct_type
        let mut facts = Facts::new();
        facts.insert(
            FACT_COMMITS_BY_TICK.to_string(),
            FactValue::CommitsByTick(sample_map.clone()),
        );
        assert_eq!(get_commits_by_tick(&facts), Some(&sample_map));

        // absent
        let facts = Facts::new();
        assert_eq!(get_commits_by_tick(&facts), None);

        // wrong_type
        let mut facts = Facts::new();
        facts.insert(FACT_COMMITS_BY_TICK.to_string(), FactValue::Int(42));
        assert_eq!(get_commits_by_tick(&facts), None);
    }

    // Ported from fact_accessors_test.go::TestGetReversedPeopleDict.
    #[test]
    fn test_get_reversed_people_dict() {
        let sample_dict = vec!["alice".to_string(), "bob".to_string()];

        // present_with_correct_type
        let mut facts = Facts::new();
        facts.insert(
            FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT.to_string(),
            FactValue::StringList(sample_dict.clone()),
        );
        assert_eq!(
            get_reversed_people_dict(&facts),
            Some(sample_dict.as_slice())
        );

        // absent
        let facts = Facts::new();
        assert_eq!(get_reversed_people_dict(&facts), None);

        // wrong_type
        let mut facts = Facts::new();
        facts.insert(
            FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT.to_string(),
            FactValue::Int(42),
        );
        assert_eq!(get_reversed_people_dict(&facts), None);
    }

    // Ported from fact_accessors_test.go::TestGetPeopleCount.
    #[test]
    fn test_get_people_count() {
        // present_with_correct_type
        let mut facts = Facts::new();
        facts.insert(
            FACT_IDENTITY_DETECTOR_PEOPLE_COUNT.to_string(),
            FactValue::Int(5),
        );
        assert_eq!(get_people_count(&facts), Some(5));

        // absent
        let facts = Facts::new();
        assert_eq!(get_people_count(&facts), None);

        // wrong_type
        let mut facts = Facts::new();
        facts.insert(
            FACT_IDENTITY_DETECTOR_PEOPLE_COUNT.to_string(),
            FactValue::Str("five".to_string()),
        );
        assert_eq!(get_people_count(&facts), None);
    }

    // Key constants must match the Go source verbatim (byte-for-byte).
    #[test]
    fn test_key_constants_verbatim() {
        assert_eq!(DEPENDENCY_FILE_DIFF, "file_diff");
        assert_eq!(DEPENDENCY_TREE_CHANGES, "changes");
        assert_eq!(DEPENDENCY_TICK, "tick");
        assert_eq!(FACT_COMMITS_BY_TICK, "TicksSinceStart.Commits");
        assert_eq!(FACT_TICK_SIZE, "TicksSinceStart.TickSize");
        assert_eq!(DEPENDENCY_BLOB_CACHE, "blob_cache");
        assert_eq!(DEPENDENCY_LANGUAGES, "languages");
        assert_eq!(DEPENDENCY_LINE_STATS, "line_stats");
        // Re-exported from cf-identity; assert the values still match Go.
        assert_eq!(
            FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT,
            "IdentityDetector.ReversedPeopleDict"
        );
        assert_eq!(
            FACT_IDENTITY_DETECTOR_PEOPLE_COUNT,
            "IdentityDetector.PeopleCount"
        );
    }

    // CachedBlob.count_lines returns ErrBinary on binary (NUL-containing) data.
    #[test]
    fn test_cached_blob_count_lines() {
        let text = CachedBlob::from_data(b"a\nb\nc".to_vec());
        assert_eq!(text.count_lines(), Ok(3));

        let text_trailing_nl = CachedBlob::from_data(b"a\nb\n".to_vec());
        assert_eq!(text_trailing_nl.count_lines(), Ok(2));

        let empty = CachedBlob::from_data(Vec::new());
        assert_eq!(empty.count_lines(), Ok(0));

        let binary = CachedBlob::from_data(b"a\0b".to_vec());
        assert_eq!(binary.count_lines(), Err(ErrBinary));
    }

    #[test]
    fn test_err_binary_message() {
        assert_eq!(ErrBinary.to_string(), "binary");
    }

    #[test]
    fn test_line_stats_default() {
        let s = LineStats::default();
        assert_eq!(s.added, 0);
        assert_eq!(s.removed, 0);
        assert_eq!(s.changed, 0);
    }

    #[test]
    fn test_file_diff_data_default() {
        let d = FileDiffData::default();
        assert!(d.diffs.is_empty());
        assert_eq!(d.old_lines_of_code, 0);
        assert_eq!(d.new_lines_of_code, 0);
    }

    #[test]
    fn test_diff_operation_discriminants() {
        assert_eq!(DiffOperation::Delete as i8, -1);
        assert_eq!(DiffOperation::Equal as i8, 0);
        assert_eq!(DiffOperation::Insert as i8, 1);
    }
}

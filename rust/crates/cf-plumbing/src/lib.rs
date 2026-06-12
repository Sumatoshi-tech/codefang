//! `cf-plumbing` — core git plumbing shared types, keys, and fact accessors.
//!
//! Core git plumbing for analysis (tree diffs, blob access) bridging gitlib to
//! identity; used by the framework, analyze, and most analyzers. This crate
//! only defines:
//!
//! * dependency / fact **key constants**,
//! * shared **types** bridged from the git layer,
//! * typed **fact accessors** over the dynamic facts map.
//!
//! Because nothing in this crate emits a MACHINE-format report, there is no
//! `cf-gojson` / `cf-goyaml` routing to do here. The optional `serde` derives
//! on [`LineStats`] exist purely so that *downstream* crates (which DO route
//! through the shared report serializers) observe the exact same field names
//! and declaration order; the field order below is the byte-identity-relevant
//! invariant (pinned by `rust/tests/compat`).
//!
//! # Bridged identity constants
//!
//! The reversed people dictionary and the people-count fact keys are owned by
//! the `cf-identity` crate and re-exported here verbatim
//! ([`FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT`],
//! [`FACT_IDENTITY_DETECTOR_PEOPLE_COUNT`]) so there is a single source of
//! truth.
//!
//! # Bridged git types
//!
//! [`CachedBlob`], [`ErrBinary`], and [`Hash`] are owned conceptually by the
//! git layer (`cf-gitlib`). While `cf-gitlib` does not publish those types
//! through its crate root, this crate defines a minimal, behavior-faithful
//! bridge surface locally. Once `cf-gitlib` exports `Hash`, `CachedBlob`, and
//! the binary-blob error, the definitions below collapse into plain
//! `pub use cf_gitlib::…;` re-exports with no change to this crate's public
//! API (the names and shapes already match).

#![forbid(unsafe_code)]

use std::collections::HashMap;
use std::time::Duration;

// Re-export the two identity fact keys from their owning crate so every
// reference resolves to a single definition shared across the workspace.
pub use cf_identity::{
    FACT_IDENTITY_DETECTOR_PEOPLE_COUNT, FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT,
};

// ---------------------------------------------------------------------------
// Dependency and fact key constants (frozen contract strings).
// ---------------------------------------------------------------------------

/// Name of the dependency provided by `FileDiff`.
pub const DEPENDENCY_FILE_DIFF: &str = "file_diff";

/// Name of the dependency provided by `TreeDiff`.
pub const DEPENDENCY_TREE_CHANGES: &str = "changes";

/// Name of the dependency which `DaysSinceStart` provides — the number of ticks
/// since the first commit in the analyzed sequence.
pub const DEPENDENCY_TICK: &str = "tick";

/// Mapping between day indices and the corresponding commits.
pub const FACT_COMMITS_BY_TICK: &str = "TicksSinceStart.Commits";

/// The [`Duration`] of each tick.
pub const FACT_TICK_SIZE: &str = "TicksSinceStart.TickSize";

/// Identifies the dependency provided by `BlobCache`.
pub const DEPENDENCY_BLOB_CACHE: &str = "blob_cache";

/// Name of the dependency provided by `LanguagesDetection`.
pub const DEPENDENCY_LANGUAGES: &str = "languages";

/// Identifier of the data provided by `LinesStatsCalculator` — line statistics
/// for each file in the commit.
pub const DEPENDENCY_LINE_STATS: &str = "line_stats";

// ---------------------------------------------------------------------------
// Shared types.
// ---------------------------------------------------------------------------

/// Git object hash (git SHA-1, a fixed 20-byte array — the same layout
/// `git2`'s `Oid` uses). Used here only as the element type of the
/// commits-by-tick fact map. The byte layout (`[u8; 20]`) matches
/// `cf-gitlib`'s `Hash(pub [u8; 20])` newtype exactly, so this alias is a
/// drop-in for the `cf-gitlib` type once that crate publishes it through its
/// crate root.
pub type Hash = [u8; 20];

/// Raised in [`CachedBlob::count_lines`] when the file is binary.
///
/// Its [`std::fmt::Display`] renders exactly `"binary"`, matching
/// `cf-gitlib`'s `GitError::Binary` message (error-text contract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("binary")]
pub struct ErrBinary;

/// A single diff edit operation — the element type of [`FileDiffData::diffs`]
/// and the shape produced by the diff engine (`cf-godiff`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    /// The edit operation kind.
    pub operation: DiffOperation,
    /// The associated text span.
    pub text: String,
}

/// Diff edit operation kind. The discriminants (`Delete = -1`, `Equal = 0`,
/// `Insert = 1`) are frozen — they match the diff engine's operation codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum DiffOperation {
    /// Text was removed.
    Delete = -1,
    /// Text is unchanged.
    Equal = 0,
    /// Text was added.
    Insert = 1,
}

/// The type of the dependency provided by `FileDiff`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDiffData {
    /// The list of diff edit operations.
    pub diffs: Vec<Diff>,
    /// Lines of code in the old version of the file.
    pub old_lines_of_code: i32,
    /// Lines of code in the new version of the file.
    pub new_lines_of_code: i32,
}

/// Bridged stand-in for `cf-gitlib`'s `CachedBlob`.
///
/// Owned blob bytes plus a [`count_lines`](Self::count_lines) that returns
/// [`ErrBinary`] for binary content. The owning implementation (with `git2`
/// loading, the memoized line count, and the full `cf-textutil`
/// binary-detection) lives in `cf-gitlib::CachedBlob`; this bridge reproduces
/// the same observable contract so analyzers that only need `count_lines` can
/// build against `cf-plumbing` directly, and it is shape-compatible with a
/// future `pub use cf_gitlib::CachedBlob;` re-export.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CachedBlob {
    /// Raw blob contents (`git2`: `blob.content().to_vec()`).
    pub data: Vec<u8>,
}

impl CachedBlob {
    /// Constructs a cached blob from raw bytes.
    #[must_use]
    pub const fn from_data(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Counts the number of lines in the blob, returning [`ErrBinary`] if the
    /// content is detected as binary.
    ///
    /// Contract: binary blobs (those containing a NUL byte) yield
    /// [`ErrBinary`]; otherwise the count is the number of `\n`-terminated
    /// lines plus a trailing partial line, and an empty blob counts as zero.
    /// The authoritative binary heuristic and line counter ship with
    /// `cf-textutil` (used by `cf-gitlib`); this bridge keeps the same
    /// observable result for the common cases analyzers rely on.
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
/// Field declaration order — `added`, `removed`, `changed` — is
/// byte-identity-relevant for any downstream wrapper that serializes this
/// struct in source order; the `serde` rename attributes pin the serialized
/// field names (report-format contract).
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
// Typed accessors over the dynamic facts map.
//
// The facts map is heterogeneous: it is modeled as
// `HashMap<String, FactValue>` and each accessor returns `Option<T>`, where
// both an absent key and a type mismatch yield `None`.
// ---------------------------------------------------------------------------

/// A value stored in the analyzer facts map.
///
/// Models the heterogeneous fact map used by the pipeline. Only the variants
/// the plumbing accessors read are enumerated; downstream crates may extend
/// this enum as more fact types are needed.
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

/// The analyzer facts map.
pub type Facts = HashMap<String, FactValue>;

/// Extracts the tick duration from the facts map.
///
/// Returns [`None`] when the key is absent or holds the wrong type.
#[must_use]
pub fn get_tick_size(facts: &Facts) -> Option<Duration> {
    match facts.get(FACT_TICK_SIZE) {
        Some(FactValue::Duration(d)) => Some(*d),
        _ => None,
    }
}

/// Extracts the commits-by-tick mapping from the facts map.
///
/// Tick indices are small, so the key type is `i32`. Returns [`None`] on
/// absence or type mismatch.
#[must_use]
pub fn get_commits_by_tick(facts: &Facts) -> Option<&HashMap<i32, Vec<Hash>>> {
    match facts.get(FACT_COMMITS_BY_TICK) {
        Some(FactValue::CommitsByTick(m)) => Some(m),
        _ => None,
    }
}

/// Extracts the reversed people dictionary from the facts map.
///
/// Reads [`FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT`]. Returns [`None`] on
/// absence or type mismatch.
#[must_use]
pub fn get_reversed_people_dict(facts: &Facts) -> Option<&[String]> {
    match facts.get(FACT_IDENTITY_DETECTOR_REVERSED_PEOPLE_DICT) {
        Some(FactValue::StringList(v)) => Some(v.as_slice()),
        _ => None,
    }
}

/// Extracts the unique author count from the facts map.
///
/// Reads [`FACT_IDENTITY_DETECTOR_PEOPLE_COUNT`]. Returns [`None`] on absence
/// or type mismatch.
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

    // Mirrors reference test TestGetTickSize.
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

    // Mirrors reference test TestGetCommitsByTick.
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

    // Mirrors reference test TestGetReversedPeopleDict.
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

    // Mirrors reference test TestGetPeopleCount.
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

    // Key constants are frozen contract strings (byte-for-byte).
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
        // Re-exported from cf-identity; assert the values still match the
        // contract.
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

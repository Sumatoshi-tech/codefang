//! Per-commit and per-tick transfer types (port of `tc.go`).

use std::collections::BTreeMap;

/// A file rename detected in a single commit (Go: `RenamePair`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePair {
    /// Original path (Go: `FromName`).
    pub from_name: String,
    /// New path (Go: `ToName`).
    pub to_name: String,
}

/// Per-commit summary used for timeseries output (Go: `CommitSummary`).
///
/// JSON field names: `files_touched`, `author_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitSummary {
    /// Number of files in the coupling context (Go: `FilesTouched`).
    pub files_touched: usize,
    /// Author index (Go: `AuthorID`).
    pub author_id: usize,
}

/// Per-commit TC payload emitted by the pipeline `Consume` step
/// (Go: `CommitData`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitData {
    /// Files forming the coupling context (already context-size filtered).
    pub coupling_files: Vec<String>,
    /// File name → touch count for this commit's author.
    ///
    /// A `BTreeMap` is used for deterministic iteration; Go uses an unordered
    /// `map[string]int` but only ever stores `1` per file, and downstream
    /// accumulation is additive, so ordering does not affect results.
    pub author_files: BTreeMap<String, i64>,
    /// Rename pairs detected in this commit.
    pub renames: Vec<RenamePair>,
    /// Whether this commit incremented the author's commit count.
    pub commit_counted: bool,
}

/// Per-tick aggregated payload (Go: `TickData`).
#[derive(Debug, Clone, Default)]
pub struct TickData {
    /// `file -> otherFile -> co-occurrence count`.
    pub files: BTreeMap<String, BTreeMap<String, i64>>,
    /// Per-author file touch counts, indexed by author ID.
    pub people: Vec<BTreeMap<String, i64>>,
    /// Per-author commit counts, indexed by author ID.
    pub people_commits: Vec<i64>,
    /// Renames accumulated during this tick.
    pub renames: Vec<RenamePair>,
    /// Per-commit summaries keyed by commit hash string.
    pub commit_stats: BTreeMap<String, CommitSummary>,
}

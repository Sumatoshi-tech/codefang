//! The per-commit payload the history walk hands to the aggregator.

use std::collections::BTreeMap;

/// A file rename detected in a single commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePair {
    /// Original path.
    pub from_name: String,
    /// New path.
    pub to_name: String,
}

/// One commit's contribution: the coupling context, the author's file
/// touches and the renames, in change order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitData {
    /// Files forming the coupling context — empty for a commit above the
    /// changeset cap ([`crate::COUPLES_MAXIMUM_MEANINGFUL_CONTEXT_SIZE`]).
    pub coupling_files: Vec<String>,
    /// File name → touch count for this commit's author.
    ///
    /// A `BTreeMap` is used for deterministic iteration; only `1` is ever
    /// stored per file and downstream accumulation is additive, so ordering
    /// does not affect results.
    pub author_files: BTreeMap<String, i64>,
    /// Rename pairs detected in this commit, applied BEFORE the commit's own
    /// touches are accumulated.
    pub renames: Vec<RenamePair>,
    /// Whether this commit incremented the author's commit count.
    pub commit_counted: bool,
}

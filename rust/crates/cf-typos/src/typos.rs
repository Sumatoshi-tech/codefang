//! Typo detection data model.
//!
//! The typos analyzer is a **commit history** analyzer: it scans each commit's
//! diffs for line pairs that are within a small Levenshtein distance (default
//! 4) and, when both the removed and added focused lines contain exactly one
//! identifier, records a `(wrong -> correct)` typo-fix pair.

use crate::compat::Hash;

/// Default maximum Levenshtein distance for typo detection.
pub const DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE: i32 = 4;

/// Configuration key for the maximum Levenshtein distance.
pub const CONFIG_TYPOS_DATASET_MAXIMUM_ALLOWED_DISTANCE: &str =
    "TyposDatasetBuilder.MaximumAllowedDistance";

/// A detected typo-fix pair in source code.
///
/// Field order (wrong, correct, file, commit, line) is the wire order when a
/// typo list serializes as struct-origin records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typo {
    /// The misspelled identifier (the "before" token).
    pub wrong: String,
    /// The corrected identifier (the "after" token).
    pub correct: String,
    /// Source file (the change's `To.Name`).
    pub file: String,
    /// Commit hash in which the fix appeared.
    pub commit: Hash,
    /// Zero-based line number in the "after" file.
    pub line: i64,
}

/// Aggregated per-tick payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TickData {
    /// Detected typos for this tick.
    pub typos: Vec<Typo>,
}

/// Removes duplicate typos, keying on `"wrong|correct"` and preserving
/// first-seen order.
///
/// The dedup is intentionally only on the `wrong|correct` pair (not
/// file/commit/line) — pinned analyzer behaviour: the first occurrence of a
/// given pair wins and later occurrences are dropped.
#[must_use]
pub fn deduplicate_typos(typos: &[Typo]) -> Vec<Typo> {
    use std::collections::HashSet;

    let mut seen: HashSet<String> = HashSet::with_capacity(typos.len());
    let mut result: Vec<Typo> = Vec::with_capacity(typos.len());

    for t in typos {
        let key = format!("{}|{}", t.wrong, t.correct);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);
        result.push(t.clone());
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typo(wrong: &str, correct: &str, file: &str, line: i64) -> Typo {
        Typo {
            wrong: wrong.to_string(),
            correct: correct.to_string(),
            file: file.to_string(),
            commit: Hash::default(),
            line,
        }
    }

    #[test]
    fn dedup_keeps_first_seen_pair() {
        let input = vec![
            typo("recieve", "receive", "a.go", 1),
            typo("recieve", "receive", "b.go", 2), // dup pair -> dropped
            typo("seperate", "separate", "c.go", 3),
        ];
        let out = deduplicate_typos(&input);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], typo("recieve", "receive", "a.go", 1));
        assert_eq!(out[1], typo("seperate", "separate", "c.go", 3));
    }

    #[test]
    fn dedup_empty() {
        assert!(deduplicate_typos(&[]).is_empty());
    }

    #[test]
    fn defaults_match_contract() {
        assert_eq!(DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE, 4);
        assert_eq!(
            CONFIG_TYPOS_DATASET_MAXIMUM_ALLOWED_DISTANCE,
            "TyposDatasetBuilder.MaximumAllowedDistance"
        );
    }
}

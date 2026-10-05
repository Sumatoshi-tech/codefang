//! The typo-fix occurrence record and its pair-level deduplication.

use std::collections::HashSet;

use crate::hash::Hash;

/// Default maximum Levenshtein distance between the two identifiers of a
/// typo-fix pair (and the line prefilter bound).
pub const DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE: usize = 4;

/// One typo-fix occurrence: an identifier a commit changed into a different,
/// Levenshtein-close identifier on the same line.
///
/// Field order (wrong, correct, file, commit, line) is the wire order of the
/// per-commit ndjson records.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Typo {
    /// The misspelled identifier (the Before side).
    pub wrong: String,
    /// The corrected identifier (the After side).
    pub correct: String,
    /// Source file (the change's After name).
    pub file: String,
    /// Commit hash in which the fix appeared.
    pub commit: Hash,
    /// 1-based line number of the fix in the After file.
    pub line: i64,
}

/// Keeps the first occurrence of every distinct `(wrong, correct)` pair, in
/// input order — the `typo_list` rule: a pair fixed on several lines or in
/// several commits is listed once, attributed to its chronologically first
/// fix.
///
/// ```
/// use cf_typos::{deduplicate_typos, Hash, Typo};
///
/// let mk = |wrong: &str, file: &str| Typo {
///     wrong: wrong.to_string(),
///     correct: "receive".to_string(),
///     file: file.to_string(),
///     commit: Hash::default(),
///     line: 1,
/// };
/// // Two occurrences of the same pair in different files: the first survives.
/// let out = deduplicate_typos(&[mk("recieve", "a.go"), mk("recieve", "b.go")]);
/// assert_eq!(out.len(), 1);
/// assert_eq!(out[0].file, "a.go");
/// ```
#[must_use]
pub fn deduplicate_typos(typos: &[Typo]) -> Vec<Typo> {
    let mut seen: HashSet<(&str, &str)> = HashSet::with_capacity(typos.len());
    typos
        .iter()
        .filter(|t| seen.insert((t.wrong.as_str(), t.correct.as_str())))
        .cloned()
        .collect()
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
            typo("recieve", "receive", "b.go", 2),
            typo("seperate", "separate", "c.go", 3),
            typo("receive", "recieve", "d.go", 4),
        ];
        let out = deduplicate_typos(&input);
        assert_eq!(
            out,
            vec![
                typo("recieve", "receive", "a.go", 1),
                typo("seperate", "separate", "c.go", 3),
                typo("receive", "recieve", "d.go", 4),
            ]
        );
    }

    #[test]
    fn dedup_empty() {
        assert!(deduplicate_typos(&[]).is_empty());
    }

    #[test]
    fn default_distance_is_four() {
        assert_eq!(DEFAULT_MAXIMUM_ALLOWED_TYPO_DISTANCE, 4);
    }
}

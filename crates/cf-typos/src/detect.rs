//! The typo-fix detection rules over one changed file.
//!
//! A history walk hands this module the line diff of a Modify change plus
//! the identifiers each focused line carries (from the UAST), and gets back
//! the `(wrong, correct)` pairs the commit fixed:
//!
//! 1. [`candidate_line_pairs`] aligns a Delete run with the Insert run of the
//!    same size that immediately follows it (line *i* of each) and keeps the
//!    pairs whose whole lines are within `max_distance` edits — a prefilter
//!    that bounds the UAST work to the lines that plausibly changed one
//!    token.
//! 2. [`identifier_pair`] diffs the identifier multisets of a candidate pair:
//!    the identifiers both lines share cancel, whatever their position, and
//!    the pair is a typo fix only when exactly ONE identifier remains on each
//!    side, both are non-empty, they differ, and the identifiers themselves
//!    are within `max_distance` edits.
//!
//! So `#define N 4` → `#define N 17` is never a typo (`N` cancels, nothing
//! remains), `v := recieve()` → `v := receive()` is (`v` cancels, the pair is
//! two edits apart), and `foo()` → `completelyDifferent()` is not (the pair
//! is far apart — and the line prefilter already rejected it). An
//! identifier is one token: an empty token, or one containing whitespace
//! (the identifier nodes prose mappings such as Markdown produce for a
//! heading), is never part of a pair.

use std::collections::HashMap;

use cf_alg_levenshtein::Context;

/// One run of the line diff between the Before and After sides of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOp {
    /// `n` lines present on both sides.
    Equal(usize),
    /// `n` lines present only on the After side.
    Insert(usize),
    /// `n` lines present only on the Before side.
    Delete(usize),
}

/// A candidate line pair: 0-based indices into the Before and After line
/// vectors a Delete/Insert run pair aligned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinePair {
    /// Index of the removed line on the Before side.
    pub before: usize,
    /// Index of the added line on the After side.
    pub after: usize,
}

/// The one identifier a candidate line pair changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierChange {
    /// The identifier on the Before side (the misspelling).
    pub wrong: String,
    /// The identifier on the After side (the correction).
    pub correct: String,
}

/// Aligns every Delete run with the Insert run of the same size that
/// immediately follows it and returns the aligned pairs whose whole lines are
/// within `max_distance` edits of each other (a byte-length gap above the
/// bound short-circuits the distance). Indices outside the line vectors are
/// skipped.
///
/// ```
/// use cf_alg_levenshtein::Context;
/// use cf_typos::{candidate_line_pairs, DiffOp, LinePair};
///
/// let before: Vec<&[u8]> = vec![b"package main", b"func recieve() {}", b"func foo() {}"];
/// let after: Vec<&[u8]> = vec![b"package main", b"func receive() {}", b"func completelyDifferent() {}"];
/// let ops = [DiffOp::Equal(1), DiffOp::Delete(2), DiffOp::Insert(2)];
/// let pairs = candidate_line_pairs(&ops, &before, &after, 4, &mut Context::new());
/// // Line 1 is two edits away; line 2 is far beyond the bound.
/// assert_eq!(pairs, vec![LinePair { before: 1, after: 1 }]);
/// ```
#[must_use]
pub fn candidate_line_pairs(
    ops: &[DiffOp],
    before: &[&[u8]],
    after: &[&[u8]],
    max_distance: usize,
    lctx: &mut Context,
) -> Vec<LinePair> {
    let mut pairs = Vec::new();
    let mut cursor_before = 0usize;
    let mut cursor_after = 0usize;
    let mut pending_delete = 0usize;
    for op in ops {
        match *op {
            DiffOp::Delete(n) => {
                cursor_before += n;
                pending_delete = n;
            }
            DiffOp::Insert(n) => {
                if n == pending_delete {
                    for i in 0..n {
                        let pair = LinePair {
                            before: cursor_before - n + i,
                            after: cursor_after + i,
                        };
                        let (Some(lb), Some(la)) = (before.get(pair.before), after.get(pair.after))
                        else {
                            continue;
                        };
                        if lines_within(lb, la, max_distance, lctx) {
                            pairs.push(pair);
                        }
                    }
                }
                cursor_after += n;
                pending_delete = 0;
            }
            DiffOp::Equal(n) => {
                cursor_before += n;
                cursor_after += n;
                pending_delete = 0;
            }
        }
    }
    pairs
}

/// Whether two raw lines are within `max_distance` edits: the byte-length gap
/// is a lower bound on the distance, so a gap above the bound skips the
/// Levenshtein computation.
fn lines_within(before: &[u8], after: &[u8], max_distance: usize, lctx: &mut Context) -> bool {
    if before.len().abs_diff(after.len()) > max_distance {
        return false;
    }
    let sb = String::from_utf8_lossy(before);
    let sa = String::from_utf8_lossy(after);
    distance(lctx, &sb, &sa) <= max_distance
}

/// The Levenshtein distance as a count (the context reports an `i32` that is
/// never negative; an impossible negative reads as "infinitely far").
fn distance(lctx: &mut Context, a: &str, b: &str) -> usize {
    usize::try_from(lctx.distance(a, b)).unwrap_or(usize::MAX)
}

/// The one identifier that changed between the identifier lists of a
/// candidate line pair, or `None` when the pair is not a typo fix.
///
/// The identifiers are compared as multisets: every identifier present on
/// both sides cancels (position is irrelevant, duplicates cancel one for
/// one). The pair is a typo fix only when exactly one identifier remains on
/// each side, both are single tokens (non-empty, no whitespace), they
/// differ, and their Levenshtein distance is at most `max_distance`.
///
/// ```
/// use cf_alg_levenshtein::Context;
/// use cf_typos::{identifier_pair, IdentifierChange};
///
/// let mut lctx = Context::new();
/// let s = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
///
/// // `v := recieve()` → `v := receive()`: `v` cancels, the pair is close.
/// assert_eq!(
///     identifier_pair(&s(&["v", "recieve"]), &s(&["v", "receive"]), 4, &mut lctx),
///     Some(IdentifierChange { wrong: "recieve".into(), correct: "receive".into() })
/// );
/// // `#define N 4` → `#define N 17`: the identifier did not change.
/// assert_eq!(identifier_pair(&s(&["N"]), &s(&["N"]), 4, &mut lctx), None);
/// // A rename to something unrelated is not a typo.
/// assert_eq!(identifier_pair(&s(&["foo"]), &s(&["completelyDifferent"]), 4, &mut lctx), None);
/// ```
#[must_use]
pub fn identifier_pair(
    before: &[String],
    after: &[String],
    max_distance: usize,
    lctx: &mut Context,
) -> Option<IdentifierChange> {
    let (removed, added) = multiset_difference(before, after);
    let [wrong] = removed.as_slice() else {
        return None;
    };
    let [correct] = added.as_slice() else {
        return None;
    };
    if !is_identifier_token(wrong) || !is_identifier_token(correct) || wrong == correct {
        return None;
    }
    if distance(lctx, wrong, correct) > max_distance {
        return None;
    }
    Some(IdentifierChange {
        wrong: (*wrong).clone(),
        correct: (*correct).clone(),
    })
}

/// Whether a UAST identifier token is one identifier: non-empty and free of
/// whitespace (prose mappings emit identifier nodes spanning whole lines).
fn is_identifier_token(token: &str) -> bool {
    !token.is_empty() && !token.chars().any(char::is_whitespace)
}

/// `(only in before, only in after)` after cancelling the identifiers both
/// lists share, one occurrence for one. Each side keeps its input order.
fn multiset_difference<'a>(
    before: &'a [String],
    after: &'a [String],
) -> (Vec<&'a String>, Vec<&'a String>) {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for ident in before {
        *counts.entry(ident.as_str()).or_insert(0) += 1;
    }
    let mut added = Vec::new();
    for ident in after {
        match counts.get_mut(ident.as_str()) {
            Some(n) if *n > 0 => *n -= 1,
            _ => added.push(ident),
        }
    }
    let removed = before
        .iter()
        .filter(|ident| {
            let n = counts
                .get_mut(ident.as_str())
                .expect("every before identifier was counted");
            if *n > 0 {
                *n -= 1;
                true
            } else {
                false
            }
        })
        .collect();
    (removed, added)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|i| (*i).to_string()).collect()
    }

    fn pair(before: &[&str], after: &[&str], max: usize) -> Option<IdentifierChange> {
        identifier_pair(&s(before), &s(after), max, &mut Context::new())
    }

    fn change(wrong: &str, correct: &str) -> Option<IdentifierChange> {
        Some(IdentifierChange {
            wrong: wrong.to_string(),
            correct: correct.to_string(),
        })
    }

    #[test]
    fn close_pair_is_a_typo_fix() {
        assert_eq!(
            pair(&["recieve"], &["receive"], 4),
            change("recieve", "receive")
        );
    }

    #[test]
    fn shared_identifiers_cancel_regardless_of_position() {
        assert_eq!(
            pair(&["v", "recieve"], &["receive", "v"], 4),
            change("recieve", "receive")
        );
        // Duplicates cancel one for one.
        assert_eq!(
            pair(&["x", "x", "lenght"], &["x", "length", "x"], 4),
            change("lenght", "length")
        );
    }

    #[test]
    fn unchanged_identifier_is_not_a_typo() {
        assert_eq!(pair(&["NUM_CON_TIMES"], &["NUM_CON_TIMES"], 4), None);
        assert_eq!(pair(&[], &[], 4), None);
    }

    #[test]
    fn empty_identifier_is_not_a_typo() {
        assert_eq!(pair(&[""], &["x"], 4), None);
        assert_eq!(pair(&["x"], &[""], 4), None);
        assert_eq!(pair(&[""], &["a"], 4), None);
    }

    #[test]
    fn tokens_with_whitespace_are_not_identifiers() {
        // A Markdown heading's identifier node spans the whole line.
        assert_eq!(pair(&["# Hello\n"], &["# Hallo\n"], 4), None);
        assert_eq!(pair(&["a b"], &["a c"], 4), None);
    }

    #[test]
    fn far_pair_is_not_a_typo() {
        assert_eq!(pair(&["foo"], &["completelyDifferent"], 4), None);
    }

    #[test]
    fn pair_distance_is_bounded_by_max_distance() {
        // `recieve` → `receive` is two edits: within 2, outside 1.
        assert_eq!(
            pair(&["recieve"], &["receive"], 2),
            change("recieve", "receive")
        );
        assert_eq!(pair(&["recieve"], &["receive"], 1), None);
    }

    #[test]
    fn two_changed_identifiers_are_not_a_typo() {
        assert_eq!(pair(&["a", "recieve"], &["b", "receive"], 4), None);
        assert_eq!(pair(&["recieve"], &["receive", "extra"], 4), None);
        assert_eq!(pair(&["recieve", "extra"], &["receive"], 4), None);
    }

    fn lines<'a>(items: &[&'a str]) -> Vec<&'a [u8]> {
        items.iter().map(|l| l.as_bytes()).collect()
    }

    #[test]
    fn aligned_runs_within_the_bound_are_candidates() {
        let before = lines(&["a", "func recieve() {}", "x := 1"]);
        let after = lines(&["a", "func receive() {}", "x := 2"]);
        let ops = [DiffOp::Equal(1), DiffOp::Delete(2), DiffOp::Insert(2)];
        let pairs = candidate_line_pairs(&ops, &before, &after, 4, &mut Context::new());
        assert_eq!(
            pairs,
            vec![
                LinePair {
                    before: 1,
                    after: 1
                },
                LinePair {
                    before: 2,
                    after: 2
                }
            ]
        );
    }

    #[test]
    fn unequal_runs_and_far_lines_are_not_candidates() {
        let before = lines(&["func foo() {}", "b"]);
        let after = lines(&["func completelyDifferent() {}", "b", "c"]);
        // A Delete(1) answered by Insert(2) does not align.
        let ops = [DiffOp::Delete(1), DiffOp::Insert(2), DiffOp::Equal(1)];
        assert!(candidate_line_pairs(&ops, &before, &after, 4, &mut Context::new()).is_empty());
        // Aligned, but the lines are far apart (and the byte-length gap alone
        // already exceeds the bound).
        let ops = [DiffOp::Delete(1), DiffOp::Insert(1), DiffOp::Equal(1)];
        assert!(candidate_line_pairs(&ops, &before, &after, 4, &mut Context::new()).is_empty());
    }

    #[test]
    fn indices_past_the_line_vectors_are_skipped() {
        let before = lines(&["a"]);
        let after = lines(&["b"]);
        let ops = [DiffOp::Delete(2), DiffOp::Insert(2)];
        let pairs = candidate_line_pairs(&ops, &before, &after, 4, &mut Context::new());
        assert_eq!(
            pairs,
            vec![LinePair {
                before: 0,
                after: 0
            }]
        );
    }

    #[test]
    fn an_equal_run_breaks_the_delete_insert_adjacency() {
        let before = lines(&["a", "k"]);
        let after = lines(&["k", "b"]);
        let ops = [DiffOp::Delete(1), DiffOp::Equal(1), DiffOp::Insert(1)];
        assert!(candidate_line_pairs(&ops, &before, &after, 4, &mut Context::new()).is_empty());
    }
}

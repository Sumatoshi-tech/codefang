//! Comment sentiment scoring.
//!
//! Wraps the [VADER engine](crate::vader) with SE-domain neutralizers and
//! comment-length weighting. VADER compound scores feed machine reports, so
//! every float operation here is part of the report contract (pinned by the
//! differential gate).

use std::sync::OnceLock;

use crate::vader::SentimentIntensityAnalyzer;

/// VADER `[-1,1]` → our `[0,1]` mapping divisor.
const VADER_COMPOUND_RANGE: f64 = 2.0;

/// Default SE-domain neutralizer weight.
pub const NEUTRALIZER_WEIGHT: f64 = 0.8;

/// Default comment-length weight cap.
pub const MAX_WEIGHT_RATIO: f64 = 3.0;

/// Configurable parameters for sentiment scoring.
#[derive(Debug, Clone, Copy)]
pub struct ScorerOptions {
    /// How strongly SE-domain adjustments apply (`0`..`1`).
    pub neutralizer_weight: f64,
    /// Cap on the per-comment length weight.
    pub max_weight_ratio: f64,
}

impl Default for ScorerOptions {
    fn default() -> Self {
        Self {
            neutralizer_weight: NEUTRALIZER_WEIGHT,
            max_weight_ratio: MAX_WEIGHT_RATIO,
        }
    }
}

/// The process-wide VADER analyzer with multilingual lexicons injected
/// (a lazily-initialized singleton).
///
/// The lexicon injection is byte-deterministic regardless of initialization
/// order because only missing, non-ASCII words are added and the merge target
/// (a hash map) is keyed by word, not insertion order.
fn vader_analyzer() -> &'static SentimentIntensityAnalyzer {
    static ANALYZER: OnceLock<SentimentIntensityAnalyzer> = OnceLock::new();
    ANALYZER.get_or_init(|| {
        let mut sia = SentimentIntensityAnalyzer::new();
        inject_multilingual_lexicons(&mut sia);
        sia
    })
}

/// Adds non-ASCII multilingual entries to VADER's lexicon.
///
/// Only non-ASCII words are injected (so the English base lexicon is never
/// overridden), and only when the lower-cased word is absent.
fn inject_multilingual_lexicons(sia: &mut SentimentIntensityAnalyzer) {
    for entry in cf_sentiment_lexicons::all() {
        if is_ascii_only(entry.word) {
            continue;
        }
        let lower = entry.word.to_lowercase();
        sia.lexicon.entry(lower).or_insert(entry.valence);
    }
}

/// Returns true if all bytes in `s` are ASCII (`< 128`).
///
/// ```
/// use cf_sentiment::scorer::is_ascii_only;
///
/// assert!(is_ascii_only("plain ascii"));
/// assert!(!is_ascii_only("café"));
/// ```
#[must_use]
pub fn is_ascii_only(s: &str) -> bool {
    s.is_ascii()
}

/// Maps a VADER compound score in `[-1,1]` to `[0,1]`.
///
/// The cast to `f32` deliberately truncates precision: the result reaches a
/// 32-bit float machine-report field, and the truncation point is part of the
/// report contract.
///
/// ```
/// use cf_sentiment::scorer::vader_compound_to_score;
///
/// // Neutral maps to the midpoint; the endpoints map to 0 and 1.
/// assert_eq!(vader_compound_to_score(0.0), 0.5);
/// assert_eq!(vader_compound_to_score(1.0), 1.0);
/// assert_eq!(vader_compound_to_score(-1.0), 0.0);
/// // Out-of-range inputs clamp.
/// assert_eq!(vader_compound_to_score(5.0), 1.0);
/// ```
#[must_use]
pub fn vader_compound_to_score(compound: f64) -> f32 {
    let score = (compound + 1.0) / VADER_COMPOUND_RANGE;
    if score < 0.0 {
        return 0.0;
    }
    if score > 1.0 {
        return 1.0;
    }
    score as f32
}

/// SE-domain terms VADER misclassifies, neutralized toward `0`.
const SE_DOMAIN_NEUTRALIZERS: &[&str] = &[
    "kill", "killed", "killing", "abort", "aborted", "aborting", "fatal", "dead", "terminate",
    "terminated", "destroy", "panic", "deprecated", "obsolete", "master", "execute", "exploit",
    "conflict", "revert", "reject", "rejected", "critical",
];

/// SE-domain terms that are genuinely negative, with their valence shift.
const SE_NEGATIVE_TERMS: &[(&str, f64)] = &[
    ("hack", -0.3),
    ("hacky", -0.4),
    ("kludge", -0.5),
    ("workaround", -0.2),
    ("technical debt", -0.3),
    ("spaghetti", -0.4),
    ("awful", -0.3),
    ("terrible", -0.3),
    ("nightmare", -0.4),
    ("horrible", -0.3),
];

/// Applies SE-domain adjustment to a VADER compound score.
///
/// # Ordering note
///
/// The reference implementation iterates these term sets in nondeterministic
/// (hash-map) order, but the result is order-independent: neutralizer targets
/// are all `0`, so each contributes `(0 - compound) * n_weight` regardless of
/// order, and the negative-term shifts are summed. `adjustment` and `count`
/// therefore match exactly. This implementation uses fixed-order slices, which
/// is numerically identical (sum of the same terms) and fully deterministic.
#[must_use]
pub fn apply_se_domain_adjustment_with_weight(text: &str, compound: f64, n_weight: f64) -> f64 {
    let lower = text.to_lowercase();
    let mut adjustment = 0.0;
    let mut count = 0_i64;

    for term in SE_DOMAIN_NEUTRALIZERS {
        if lower.contains(term) {
            // shift is 0.0 for all neutralizers.
            adjustment += (0.0 - compound) * n_weight;
            count += 1;
        }
    }

    for (term, shift) in SE_NEGATIVE_TERMS {
        if lower.contains(term) {
            adjustment += shift;
            count += 1;
        }
    }

    if count == 0 {
        return compound;
    }

    let adjusted = compound + adjustment / count as f64;
    adjusted.clamp(-1.0, 1.0)
}

/// Returns a sentiment score in `[0,1]` for `comments` using default options.
///
/// ```
/// use cf_sentiment::scorer::compute_sentiment;
///
/// // No comments → neutral 0.0.
/// assert_eq!(compute_sentiment(&[]), 0.0);
///
/// // A clearly positive comment scores above the 0.5 neutral midpoint.
/// let score = compute_sentiment(&["This is a great, clean fix!".to_string()]);
/// assert!(score > 0.5, "score = {score}");
/// ```
#[must_use]
pub fn compute_sentiment(comments: &[String]) -> f32 {
    compute_sentiment_with_options(comments, ScorerOptions::default())
}

/// Returns a sentiment score with configurable parameters.
#[must_use]
pub fn compute_sentiment_with_options(comments: &[String], opts: ScorerOptions) -> f32 {
    if comments.is_empty() {
        return 0.0;
    }

    let analyzer = vader_analyzer();
    let n_weight = opts.neutralizer_weight;
    let max_wr = opts.max_weight_ratio;

    let mut weighted_sum = 0.0_f64;
    let mut total_weight = 0.0_f64;

    let avg_len = average_comment_length(comments);

    for c in comments {
        let c = c.trim();
        if c.is_empty() {
            continue;
        }

        let scores = analyzer.polarity_scores(c);
        let adjusted = apply_se_domain_adjustment_with_weight(c, scores.compound, n_weight);

        let weight = comment_weight_with_max(c.len(), avg_len, max_wr);
        weighted_sum += f64::from(vader_compound_to_score(adjusted)) * weight;
        total_weight += weight;
    }

    if total_weight == 0.0 {
        return 0.0;
    }

    (weighted_sum / total_weight) as f32
}

/// Mean byte-length of non-empty (trimmed) comments.
#[must_use]
pub fn average_comment_length(comments: &[String]) -> f64 {
    let mut total = 0_usize;
    let mut count = 0_usize;
    for c in comments {
        let c = c.trim();
        if c.is_empty() {
            continue;
        }
        total += c.len();
        count += 1;
    }
    if count == 0 {
        return 1.0;
    }
    total as f64 / count as f64
}

/// Length weight capped at `max_ratio`.
#[must_use]
pub fn comment_weight_with_max(length: usize, avg_length: f64, max_ratio: f64) -> f64 {
    if avg_length <= 0.0 {
        return 1.0;
    }
    let ratio = length as f64 / avg_length;
    ratio.min(max_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{SENTIMENT_NEGATIVE_THRESHOLD, SENTIMENT_POSITIVE_THRESHOLD};

    const FLOAT_DELTA: f64 = 0.01;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn compute_sentiment_empty() {
        assert!((f64::from(compute_sentiment(&[]))).abs() < FLOAT_DELTA);
    }

    #[test]
    fn compute_sentiment_whitespace_only() {
        assert!((f64::from(compute_sentiment(&s(&["  ", "\t", ""])))).abs() < FLOAT_DELTA);
    }

    #[test]
    fn compute_sentiment_positive() {
        let score = compute_sentiment(&s(&["This is a great fix!"]));
        assert!(f64::from(score) >= SENTIMENT_POSITIVE_THRESHOLD, "score = {score}");
    }

    #[test]
    fn compute_sentiment_negative() {
        let score = compute_sentiment(&s(&["This code is broken and terrible."]));
        assert!(f64::from(score) <= SENTIMENT_NEGATIVE_THRESHOLD, "score = {score}");
    }

    #[test]
    fn compute_sentiment_neutral() {
        let score = compute_sentiment(&s(&["The function handles input validation."]));
        assert!(f64::from(score) > SENTIMENT_NEGATIVE_THRESHOLD);
        assert!(f64::from(score) < SENTIMENT_POSITIVE_THRESHOLD);
    }

    #[test]
    fn compute_sentiment_mixed() {
        let score = compute_sentiment(&s(&["This is great!", "This is broken."]));
        assert!(f64::from(score) > SENTIMENT_NEGATIVE_THRESHOLD);
        assert!(f64::from(score) < SENTIMENT_POSITIVE_THRESHOLD);
    }

    #[test]
    fn compute_sentiment_multiple_comments() {
        let score = compute_sentiment(&s(&["good work", "nice refactor", "clean code"]));
        assert!(f64::from(score) >= SENTIMENT_POSITIVE_THRESHOLD, "score = {score}");
    }

    #[test]
    fn compute_sentiment_heavy_negative() {
        let score = compute_sentiment(&s(&["This is terrible awful horrible broken bug hack"]));
        assert!(f64::from(score) <= SENTIMENT_NEGATIVE_THRESHOLD, "score = {score}");
    }

    #[test]
    fn compute_sentiment_se_neutral_terms() {
        let cases = [
            "Kill the background process when idle",
            "Abort the transaction on timeout",
            "Log fatal error and exit gracefully",
            "Terminate the worker thread after cleanup",
            "This deprecated API will be removed next version",
            "Panic handler catches unrecoverable errors",
            "Execute the shell command with the given arguments",
        ];
        for c in cases {
            let score = compute_sentiment(&s(&[c]));
            assert!(
                f64::from(score) > SENTIMENT_NEGATIVE_THRESHOLD,
                "SE technical term should not be negative: {c} (score {score})"
            );
        }
    }

    #[test]
    fn compute_sentiment_se_negative_terms() {
        let cases = [
            "This is a really hacky workaround for the issue",
            "This spaghetti code needs serious refactoring",
            "This codebase is a nightmare to maintain",
        ];
        for c in cases {
            let score = compute_sentiment(&s(&[c]));
            assert!(
                f64::from(score) <= SENTIMENT_NEGATIVE_THRESHOLD,
                "SE negative term should be negative: {c} (score {score})"
            );
        }
    }

    #[test]
    fn compute_sentiment_length_weighting() {
        let short = "bad";
        let long = "This function is well-designed and implements the algorithm correctly with great readability";
        let score = compute_sentiment(&s(&[short, long]));
        let long_only = compute_sentiment(&s(&[long]));
        assert!(f64::from(score) > SENTIMENT_NEGATIVE_THRESHOLD);
        assert!((f64::from(long_only) - f64::from(score)).abs() < 0.15);
    }

    #[test]
    fn apply_se_no_terms() {
        let r = apply_se_domain_adjustment_with_weight("simple regular comment", 0.5, NEUTRALIZER_WEIGHT);
        assert!((r - 0.5).abs() < FLOAT_DELTA);
    }

    #[test]
    fn apply_se_with_neutralizer() {
        let r = apply_se_domain_adjustment_with_weight("kill the process", -0.6, NEUTRALIZER_WEIGHT);
        assert!(r > -0.6);
    }

    #[test]
    fn apply_se_with_negative_term() {
        let r = apply_se_domain_adjustment_with_weight("this is a terrible hack", 0.0, NEUTRALIZER_WEIGHT);
        assert!(r < 0.0);
    }

    #[test]
    fn apply_se_clamps_bounds() {
        let r = apply_se_domain_adjustment_with_weight(
            "nightmare spaghetti awful terrible hack kludge",
            0.9,
            NEUTRALIZER_WEIGHT,
        );
        assert!(r >= -1.0);
        assert!(r <= 1.0);
    }

    #[test]
    fn vader_compound_to_score_boundaries() {
        assert!((f64::from(vader_compound_to_score(-1.0))).abs() < FLOAT_DELTA);
        assert!((f64::from(vader_compound_to_score(0.0)) - 0.5).abs() < FLOAT_DELTA);
        assert!((f64::from(vader_compound_to_score(1.0)) - 1.0).abs() < FLOAT_DELTA);
        assert!((f64::from(vader_compound_to_score(-2.0))).abs() < FLOAT_DELTA);
        assert!((f64::from(vader_compound_to_score(2.0)) - 1.0).abs() < FLOAT_DELTA);
    }

    #[test]
    fn comment_weight() {
        let max = ScorerOptions::default().max_weight_ratio;
        assert!((comment_weight_with_max(50, 50.0, max) - 1.0).abs() < FLOAT_DELTA);
        assert!((comment_weight_with_max(100, 50.0, max) - 2.0).abs() < FLOAT_DELTA);
        assert!((comment_weight_with_max(500, 50.0, max) - MAX_WEIGHT_RATIO).abs() < FLOAT_DELTA);
        assert!((comment_weight_with_max(50, 0.0, max) - 1.0).abs() < FLOAT_DELTA);
    }

    #[test]
    fn compute_sentiment_multilingual() {
        let ru_pos = "\u{43e}\u{442}\u{43b}\u{438}\u{447}\u{43d}\u{43e} \u{443}\u{441}\u{43f}\u{435}\u{448}\u{43d}\u{43e}";
        let ru_neg = "\u{43f}\u{43b}\u{43e}\u{445}\u{43e} \u{43e}\u{448}\u{438}\u{431}\u{43a}\u{430} \u{443}\u{436}\u{430}\u{441}\u{43d}\u{43e}";
        let pos = compute_sentiment(&s(&[ru_pos]));
        let neg = compute_sentiment(&s(&[ru_neg]));
        assert!(f64::from(pos) > SENTIMENT_NEGATIVE_THRESHOLD);
        assert!(f64::from(neg) < SENTIMENT_POSITIVE_THRESHOLD);
    }

    #[test]
    fn inject_multilingual_grows_lexicon() {
        let analyzer = vader_analyzer();
        assert!(analyzer.lexicon.len() > 7500, "lexicon size = {}", analyzer.lexicon.len());
    }

    #[test]
    fn is_ascii_only_cases() {
        assert!(is_ascii_only("hello"));
        assert!(is_ascii_only("fix123"));
        assert!(is_ascii_only(""));
        assert!(!is_ascii_only("\u{43f}\u{43b}\u{43e}\u{445}\u{43e}"));
        assert!(!is_ascii_only("\u{597d}"));
    }

    #[test]
    fn average_comment_length_cases() {
        assert!((average_comment_length(&[]) - 1.0).abs() < FLOAT_DELTA);
        assert!((average_comment_length(&s(&["hello"])) - 5.0).abs() < FLOAT_DELTA);
        assert!((average_comment_length(&s(&["  ", "\t"])) - 1.0).abs() < FLOAT_DELTA);
        assert!((average_comment_length(&s(&["ab", "abcd", "abcdef"])) - 4.0).abs() < FLOAT_DELTA);
    }
}

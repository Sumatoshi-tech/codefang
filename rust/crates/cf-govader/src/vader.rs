//! Core VADER scoring functions. Direct port of `vader.go`.

use crate::constants::{
    ALPHA_DEFAULT, BUT_SCALE, EP_AMPLIFY_SCALE, MAX_EP, MAX_QM, NEGATION_SCALE, N_SCALAR,
    QM_AMPLIFY_SCALE,
};
use crate::Sentiment;

/// Determines if input contains negation words (optionally `n't` contractions).
///
/// Mirrors Go `negated`.
fn negated(input_words: &[&str], include_nt: bool, negate_list: &[String]) -> bool {
    for x in input_words {
        if negate_list.iter().any(|n| n == x) {
            return true;
        }
    }
    if include_nt {
        for w in input_words {
            if w.contains("n't") {
                return true;
            }
        }
    }
    false
}

/// Normalizes `score` into `[-1, 1]` using `alpha`. Mirrors Go `normalize`.
fn normalize(score: f64, alpha: f64) -> f64 {
    let norm_score = score / ((score * score) + alpha).sqrt();
    if norm_score < -1.0 {
        -1.0
    } else if norm_score > 1.0 {
        1.0
    } else {
        norm_score
    }
}

/// Normalizes with the default alpha. Mirrors Go `normalizeDefault`.
fn normalize_default(score: f64) -> f64 {
    normalize(score, ALPHA_DEFAULT)
}

/// Splits sentiments into positive sum, negative sum, neutral count.
///
/// Mirrors Go `siftSentimentScores`.
fn sift_sentiment_scores(sentiments: &[f64]) -> (f64, f64, i64) {
    let mut pos_sum = 0.0;
    let mut neg_sum = 0.0;
    let mut neu_count = 0_i64;
    for &v in sentiments {
        if v > 0.0 {
            pos_sum += v + 1.0;
        }
        if v < 0.0 {
            neg_sum += v - 1.0;
        }
        if v == 0.0 {
            neu_count += 1;
        }
    }
    (pos_sum, neg_sum, neu_count)
}

/// Combined `!`/`?` punctuation emphasis. Mirrors Go `punctuationEmphasis`.
fn punctuation_emphasis(text: &str) -> f64 {
    amplify_ep(text) + amplify_qm(text)
}

/// Exclamation-point amplifier. Mirrors Go `amplifyEP`.
fn amplify_ep(text: &str) -> f64 {
    let mut ep_count = text.matches('!').count() as i64;
    if ep_count > MAX_EP {
        ep_count = MAX_EP;
    }
    ep_count as f64 * EP_AMPLIFY_SCALE
}

/// Question-mark amplifier. Mirrors Go `amplifyQM`.
fn amplify_qm(text: &str) -> f64 {
    let qm_count = text.matches('?').count() as i64;
    if qm_count > 1 {
        if qm_count <= 3 {
            return qm_count as f64 * QM_AMPLIFY_SCALE;
        }
        return MAX_QM;
    }
    0.0
}

/// Applies negation context. Mirrors Go `negationCheck`.
pub(crate) fn negation_check(
    valence: f64,
    wel: &[String], // wordsAndEmoticonsLower
    starti: usize,
    i: usize,
    negate_list: &[String],
) -> f64 {
    let mut new_valence = valence;
    if starti == 0 {
        if negated(&[wel[i - (starti + 1)].as_str()], true, negate_list) {
            new_valence *= N_SCALAR;
        }
    }
    if starti == 1 {
        if wel[i - 2] == "never" && (wel[i - 1] == "so" || wel[i - 1] == "this") {
            new_valence = valence * NEGATION_SCALE;
        } else if wel[i - 2] == "without" && wel[i - 1] == "doubt" {
            new_valence = valence;
        } else if negated(&[wel[i - (starti + 1)].as_str()], true, negate_list) {
            new_valence = valence * N_SCALAR;
        }
    }
    if starti == 2 {
        if wel[i - 3] == "never"
            && ((wel[i - 2] == "so" || wel[i - 2] == "this")
                || (wel[i - 1] == "so" || wel[i - 1] == "this"))
        {
            new_valence = valence * NEGATION_SCALE;
        } else if wel[i - 3] == "without" && (wel[i - 2] == "doubt" || wel[i - 1] == "doubt") {
            new_valence = valence;
        } else if negated(&[wel[i - (starti + 1)].as_str()], true, negate_list) {
            new_valence = valence * N_SCALAR;
        }
    }
    new_valence
}

/// Applies the contrastive-conjunction "but" adjustment in place.
///
/// Mirrors Go `butCheck`: sentiments before the first "but" are scaled by
/// `(1 - butScale)`, those after by `(1 + butScale)`.
pub(crate) fn but_check(wel: &[String], sentiments: &mut [f64]) {
    let Some(bi) = wel.iter().position(|w| w == "but") else {
        return;
    };
    for (i, s) in sentiments.iter_mut().enumerate() {
        if i < bi {
            *s = (1.0 - BUT_SCALE) * *s;
        }
        if i > bi {
            *s = (1.0 + BUT_SCALE) * *s;
        }
    }
}

/// Produces the final [`Sentiment`] from per-token valences and the text.
///
/// Mirrors Go `scoreValence`. The Go code sums the slice via `gonum mat.Sum`,
/// which is a plain left-to-right float sum — reproduced here with `iter().sum()`
/// over `f64` (same operand order).
pub(crate) fn score_valence(sentiments: &[f64], text: &str) -> Sentiment {
    let mut sentiment = Sentiment::default();

    if !sentiments.is_empty() {
        let mut sum_s: f64 = sentiments.iter().sum();
        let punct_emph_amplifier = punctuation_emphasis(text);
        if sum_s > 0.0 {
            sum_s += punct_emph_amplifier;
        } else if sum_s < 0.0 {
            sum_s -= punct_emph_amplifier;
        }
        sentiment.compound = normalize_default(sum_s);

        let (mut pos_sum, mut neg_sum, neu_count) = sift_sentiment_scores(sentiments);
        if pos_sum > neg_sum.abs() {
            pos_sum += punct_emph_amplifier;
        } else if pos_sum < neg_sum.abs() {
            neg_sum -= punct_emph_amplifier;
        }
        let total = pos_sum + neg_sum.abs() + neu_count as f64;
        sentiment.positive = (pos_sum / total).abs();
        sentiment.negative = (neg_sum / total).abs();
        sentiment.neutral = (neu_count as f64 / total).abs();
    }

    sentiment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_clamps() {
        assert!((normalize_default(0.0)).abs() < 1e-12);
        assert!(normalize_default(1000.0) <= 1.0);
        assert!(normalize_default(-1000.0) >= -1.0);
    }

    #[test]
    fn but_check_scales_around_but() {
        let wel: Vec<String> = ["good", "but", "bad"].iter().map(|s| s.to_string()).collect();
        let mut s = vec![2.0, 0.0, -2.0];
        but_check(&wel, &mut s);
        assert!((s[0] - 1.0).abs() < 1e-12); // (1-0.5)*2
        assert!((s[2] - -3.0).abs() < 1e-12); // (1+0.5)*-2
    }

    #[test]
    fn amplify_punct() {
        assert!((amplify_ep("a!!!") - 3.0 * EP_AMPLIFY_SCALE).abs() < 1e-12);
        // capped at MAX_EP.
        assert!((amplify_ep("!!!!!!!") - MAX_EP as f64 * EP_AMPLIFY_SCALE).abs() < 1e-12);
        assert!(amplify_qm("a?").abs() < 1e-12); // single ? => 0
        assert!((amplify_qm("a??") - 2.0 * QM_AMPLIFY_SCALE).abs() < 1e-12);
    }
}

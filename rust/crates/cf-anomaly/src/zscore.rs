//! Rolling Z-score computation.
//!
//! Port of `rollingZScores` in `internal/analyzers/anomaly/statistics.go`. For
//! each index `i`, computes the Z-score of `values[i]` relative to the trailing
//! window ending at `i` (inclusive). When the trailing window has zero variance,
//! the Z-score is 0.

use crate::statistics::mean_std;

/// Returns, for each index `i`, the Z-score of `values[i]` relative to the
/// trailing `window` values ending at `i` (inclusive).
///
/// Mirrors Go's `rollingZScores` exactly: the window start is
/// `max(0, i - window + 1)`, the population mean/std come from [`mean_std`], and a
/// zero-variance window yields a Z-score of `0.0`.
#[must_use]
pub fn rolling_zscores(values: &[f64], window: usize) -> Vec<f64> {
    let n = values.len();
    let mut result = vec![0.0_f64; n];
    if n == 0 {
        return result;
    }
    for i in 0..n {
        // Go: start := i - window + 1; if start < 0 { start = 0 }.
        // (i + 1).saturating_sub(window) == max(0, i - window + 1) for usize.
        let start = (i + 1).saturating_sub(window);
        let window_vals = &values[start..=i];
        let (mean, std) = mean_std(window_vals);
        if std == 0.0 {
            result[i] = 0.0;
            continue;
        }
        result[i] = (values[i] - mean) / std;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        assert!(rolling_zscores(&[], 5).is_empty());
    }

    #[test]
    fn first_element_is_zero() {
        // A single-element trailing window always has zero variance -> z 0.
        let z = rolling_zscores(&[10.0, 20.0, 30.0], 1);
        assert_eq!(z, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn flags_an_outlier_positive() {
        // Flat history then a spike: the spike has a large positive z-score.
        let z = rolling_zscores(&[1.0, 1.0, 1.0, 10.0], 4);
        assert_eq!(z[0], 0.0);
        assert!(z[3] > 1.0, "spike z-score should be large positive, got {}", z[3]);
    }
}

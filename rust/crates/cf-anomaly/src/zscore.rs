//! Trailing-window Z-score computation.
//!
//! Port of `ComputeZScores` in `internal/analyzers/anomaly/zscore.go`. For each
//! index `i`, the Z-score is computed relative to the trailing window ending at
//! (and including) `i`: `values[max(0, i-window+1) ..= i]`.
//!
//! Edge rules mirror Go exactly:
//! * fewer than [`MIN_SAMPLES_FOR_Z_SCORE`] samples in the trailing window → `0`;
//! * zero standard deviation and the value equals the mean → `0`;
//! * zero standard deviation but the value differs → [`Z_SCORE_SENTINEL_HIGH`]
//!   (an unbounded-spike marker).

use cf_alg_stats::mean_std_dev;

/// Minimum trailing-window samples before a Z-score is meaningful.
///
/// Mirrors Go `minSamplesForZScore`.
pub const MIN_SAMPLES_FOR_Z_SCORE: usize = 3;

/// Sentinel Z-score for a zero-variance trailing window whose current value
/// differs from the window mean. Mirrors Go `zScoreSentinelHigh` (`100.0`) and
/// `cf_alg_stats`'s `ZSCORE_MAX_SENTINEL`.
pub const Z_SCORE_SENTINEL_HIGH: f64 = 100.0;

/// Returns, for each index `i`, the trailing-window Z-score of `values[i]`.
///
/// Mirrors Go `ComputeZScores`. The window start is `max(0, i - window + 1)`; the
/// population mean/std come from [`cf_alg_stats::mean_std_dev`]; the edge rules
/// above are applied verbatim.
#[must_use]
pub fn compute_z_scores(values: &[f64], window: usize) -> Vec<f64> {
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

        if window_vals.len() < MIN_SAMPLES_FOR_Z_SCORE {
            result[i] = 0.0;
            continue;
        }

        let (mean, std) = mean_std_dev(window_vals);
        if std == 0.0 {
            result[i] = if (values[i] - mean).abs() > 0.0 {
                Z_SCORE_SENTINEL_HIGH
            } else {
                0.0
            };
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
        assert!(compute_z_scores(&[], 5).is_empty());
    }

    #[test]
    fn below_min_samples_is_zero() {
        // Mirrors Go TestComputeZScores_WindowOfOne: a window never reaching the
        // 3-sample minimum yields all zeros.
        let z = compute_z_scores(&[10.0, 20.0, 30.0], 1);
        assert_eq!(z, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn zero_stddev_equal_is_zero() {
        // Mirrors Go TestComputeZScores_ZeroStdDev: flat history, value equals
        // the mean -> z 0 once enough samples accumulate.
        let z = compute_z_scores(&[5.0, 5.0, 5.0, 5.0], 4);
        assert_eq!(z, vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn zero_stddev_with_diff_uses_sentinel() {
        // Mirrors Go TestComputeZScores_SentinelOnZeroStdDevWithDiff: a flat
        // trailing window then a differing value -> sentinel 100.0.
        let z = compute_z_scores(&[1.0, 1.0, 1.0, 9.0], 4);
        assert_eq!(z[3], Z_SCORE_SENTINEL_HIGH);
    }

    #[test]
    fn basic_spike_positive() {
        // Mirrors Go TestComputeZScores_BasicSpike: a spike after varied history
        // yields a large positive (finite) z-score.
        let z = compute_z_scores(&[1.0, 2.0, 3.0, 2.0, 50.0], 5);
        assert!(z[4] > 1.0, "spike z-score should be large positive, got {}", z[4]);
    }
}

//! Trailing-window Z-score computation.
//!
//! For each index `i`, the Z-score is computed relative to the trailing
//! window of values **before** `i`: `values[max(0, i-window) .. i]`
//! (exclusive of `i`).
//!
//! Edge rules (reference-implementation behavior, pinned by the differential
//! gate):
//! * the window for `i == 0` is empty → Z-score `0`;
//! * zero standard deviation and the value equals the mean → `0`;
//! * zero standard deviation but the value differs → the signed sentinel
//!   `copysign(`[`Z_SCORE_MAX_SENTINEL`]`, diff)` (so it may be `-100.0`).

use cf_alg_stats::{mean_std_dev, Z_SCORE_MAX_SENTINEL};

/// Sentinel magnitude (`100.0`) for a zero-variance trailing window whose
/// current value differs from the window mean. Re-exported from
/// `cf_alg_stats`; the emitted value is signed via `copysign`.
pub use cf_alg_stats::Z_SCORE_MAX_SENTINEL as Z_SCORE_SENTINEL;

/// Returns, for each index `i`, the trailing-window Z-score of `values[i]`.
///
/// The operation order is part of the float contract:
/// * `window` is clamped up to `1` when `< 1`;
/// * the window is `values[max(0, i-window) .. i]` (**exclusive** of `i`);
/// * an empty window (`i == 0`) yields `0`;
/// * mean/std come from [`cf_alg_stats::mean_std_dev`];
/// * zero-std yields `0` when the value equals the mean, else
///   `copysign(Z_SCORE_MAX_SENTINEL, value - mean)`.
///
/// ```
/// use cf_anomaly::zscore::{compute_z_scores, Z_SCORE_SENTINEL};
///
/// // Index 0 always has an empty trailing window → 0.
/// let z = compute_z_scores(&[1.0, 1.0, 1.0, 9.0], 3);
/// assert_eq!(z[0], 0.0);
/// // A flat window then a larger value → +sentinel (zero variance).
/// assert_eq!(z[3], Z_SCORE_SENTINEL);
///
/// // A smaller value than a flat window → -sentinel (copysign).
/// let z2 = compute_z_scores(&[9.0, 9.0, 9.0, 1.0], 3);
/// assert_eq!(z2[3], -Z_SCORE_SENTINEL);
///
/// // Empty input → empty output.
/// assert!(compute_z_scores(&[], 5).is_empty());
/// ```
#[must_use]
pub fn compute_z_scores(values: &[f64], window: usize) -> Vec<f64> {
    let count = values.len();
    if count == 0 {
        return Vec::new();
    }

    // A window below 1 is clamped to 1; usize is >= 0, so only 0 needs it.
    let window = window.max(1);

    let mut scores = vec![0.0_f64; count];

    for i in 0..count {
        let start = i.saturating_sub(window); // max(0, i-window)
        let window_slice = &values[start..i]; // EXCLUSIVE of i.

        if window_slice.is_empty() {
            scores[i] = 0.0;
            continue;
        }

        let (mean, stddev) = mean_std_dev(window_slice);

        if stddev == 0.0 {
            let diff = values[i] - mean;
            scores[i] = if diff == 0.0 {
                0.0
            } else {
                Z_SCORE_MAX_SENTINEL.copysign(diff)
            };
            continue;
        }

        scores[i] = (values[i] - mean) / stddev;
    }

    scores
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact contract values (sentinels, exact zeros) are the point
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        assert!(compute_z_scores(&[], 5).is_empty());
    }

    #[test]
    fn first_index_window_is_empty_zero() {
        // i == 0 has an empty trailing window -> 0.
        let z = compute_z_scores(&[10.0, 20.0, 30.0], 3);
        assert_eq!(z[0], 0.0);
    }

    #[test]
    fn zero_stddev_equal_is_zero() {
        // Mirrors reference test TestComputeZScores_ZeroStdDev: flat trailing
        // window, value equals the mean -> 0.
        let z = compute_z_scores(&[5.0, 5.0, 5.0, 5.0], 3);
        assert_eq!(z, vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn zero_stddev_with_diff_uses_signed_sentinel() {
        // Mirrors reference test TestComputeZScores_SentinelOnZeroStdDevWithDiff:
        // a flat trailing window then a larger value -> +sentinel.
        let z = compute_z_scores(&[1.0, 1.0, 1.0, 9.0], 3);
        assert_eq!(z[3], Z_SCORE_MAX_SENTINEL);
        // A smaller value than a flat window -> -sentinel (copysign).
        let z2 = compute_z_scores(&[9.0, 9.0, 9.0, 1.0], 3);
        assert_eq!(z2[3], -Z_SCORE_MAX_SENTINEL);
    }

    #[test]
    fn basic_spike_finite_positive() {
        // Trailing window excludes i; a spike after varied history gives a large
        // finite positive z-score.
        let z = compute_z_scores(&[1.0, 2.0, 3.0, 2.0, 50.0], 4);
        assert!(z[4] > 1.0, "spike z-score should be large positive, got {}", z[4]);
    }

    #[test]
    fn window_clamped_to_one() {
        // window 0 clamps to 1: each i sees exactly the single prior value, which
        // has zero variance, so any change yields the signed sentinel.
        let z = compute_z_scores(&[1.0, 2.0], 0);
        assert_eq!(z[0], 0.0);
        assert_eq!(z[1], Z_SCORE_MAX_SENTINEL);
    }
}

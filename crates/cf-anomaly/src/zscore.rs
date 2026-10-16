//! Trailing-window Z-score computation.
//!
//! For each index `i`, the Z-score is computed relative to the trailing
//! window of values **before** `i`: `values[max(0, i-window) .. i]`
//! (exclusive of `i`), with the population standard deviation. A window
//! shorter than `window` (the first ticks of a history) is the trailing
//! window that exists.
//!
//! Edge rules:
//! * fewer than [`MIN_WINDOW_SAMPLES`] observations in the window → `0`
//!   (one observation has no variance to speak of);
//! * zero standard deviation and the value equals the mean → `0`;
//! * zero standard deviation and the value differs → the signed sentinel
//!   `copysign(`[`Z_SCORE_SENTINEL`]`, diff)`.

use cf_alg_stats::mean_std_dev;

/// Sentinel magnitude (`100.0`) for a zero-variance trailing window whose
/// current value differs from the window mean. Re-exported from
/// `cf_alg_stats`; the emitted value is signed via `copysign`.
pub use cf_alg_stats::Z_SCORE_MAX_SENTINEL as Z_SCORE_SENTINEL;

/// The observations a trailing window needs before it yields a Z-score: a
/// single observation has no spread, so neither a finite score nor the
/// zero-variance sentinel is meaningful.
pub const MIN_WINDOW_SAMPLES: usize = 2;

/// Returns, for each index `i`, the trailing-window Z-score of `values[i]`.
///
/// * the window is `values[max(0, i-window) .. i]` (**exclusive** of `i`),
///   `window` clamped up to `1`;
/// * fewer than [`MIN_WINDOW_SAMPLES`] observations yield `0`;
/// * mean/std come from [`cf_alg_stats::mean_std_dev`] (population);
/// * zero-std yields `0` when the value equals the mean, else
///   `copysign(Z_SCORE_SENTINEL, value - mean)`.
///
/// ```
/// use cf_anomaly::zscore::{compute_z_scores, Z_SCORE_SENTINEL};
///
/// // Index 0 (no window) and index 1 (one observation) → 0.
/// let z = compute_z_scores(&[1.0, 5.0, 1.0, 1.0, 9.0], 3);
/// assert_eq!(z[0], 0.0);
/// assert_eq!(z[1], 0.0);
/// // A flat window of two or more then a larger value → +sentinel.
/// let z = compute_z_scores(&[1.0, 1.0, 1.0, 9.0], 3);
/// assert_eq!(z[3], Z_SCORE_SENTINEL);
///
/// // A smaller value than a flat window → -sentinel (copysign).
/// let z2 = compute_z_scores(&[9.0, 9.0, 9.0, 1.0], 3);
/// assert_eq!(z2[3], -Z_SCORE_SENTINEL);
///
/// // Hand computation: window [10, 12] has mean 11 and population std 1.
/// let z3 = compute_z_scores(&[10.0, 12.0, 200.0], 20);
/// assert_eq!(z3[2], 189.0);
///
/// // Empty input → empty output.
/// assert!(compute_z_scores(&[], 5).is_empty());
/// ```
#[must_use]
pub fn compute_z_scores(values: &[f64], window: usize) -> Vec<f64> {
    let window = window.max(1);
    values
        .iter()
        .enumerate()
        .map(|(i, &value)| {
            let start = i.saturating_sub(window);
            z_score(value, &values[start..i])
        })
        .collect()
}

/// The Z-score of `value` against `window` (the observations before it).
fn z_score(value: f64, window: &[f64]) -> f64 {
    if window.len() < MIN_WINDOW_SAMPLES {
        return 0.0;
    }
    let (mean, stddev) = mean_std_dev(window);
    let diff = value - mean;
    if stddev == 0.0 {
        if diff == 0.0 {
            0.0
        } else {
            Z_SCORE_SENTINEL.copysign(diff)
        }
    } else {
        diff / stddev
    }
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
    fn first_two_indices_have_no_usable_window() {
        // i == 0: empty window; i == 1: one observation -> both 0.
        let z = compute_z_scores(&[10.0, 20.0, 30.0], 3);
        assert_eq!(z[0], 0.0);
        assert_eq!(z[1], 0.0);
    }

    #[test]
    fn one_observation_never_yields_the_sentinel() {
        // The old rule flagged index 1 with ±100 after a single sample.
        let z = compute_z_scores(&[10.0, 12.0], 20);
        assert_eq!(z, vec![0.0, 0.0]);
    }

    #[test]
    fn zero_stddev_equal_is_zero() {
        let z = compute_z_scores(&[5.0, 5.0, 5.0, 5.0], 3);
        assert_eq!(z, vec![0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn zero_stddev_with_diff_uses_signed_sentinel() {
        let z = compute_z_scores(&[1.0, 1.0, 1.0, 9.0], 3);
        assert_eq!(z[3], Z_SCORE_SENTINEL);
        let z2 = compute_z_scores(&[9.0, 9.0, 9.0, 1.0], 3);
        assert_eq!(z2[3], -Z_SCORE_SENTINEL);
    }

    #[test]
    fn hand_computed_over_the_documented_window() {
        // Ten quiet ticks alternating 10/12: mean 11, population std 1; the
        // busy tick's 190 scores (190 - 11) / 1 over the default window.
        let mut values: Vec<f64> = (0..10)
            .map(|i| if i % 2 == 0 { 10.0 } else { 12.0 })
            .collect();
        values.push(190.0);
        let z = compute_z_scores(&values, 20);
        assert_eq!(z[10], 179.0);
        // A three-tick window scores against [12, 10, 12]: mean 34/3,
        // population std sqrt(8/9).
        let z3 = compute_z_scores(&values, 3);
        let mean = 34.0 / 3.0;
        let std = (8.0_f64 / 9.0).sqrt();
        assert!((z3[10] - (190.0 - mean) / std).abs() < 1e-9, "{}", z3[10]);
        // Index 2 sees [10, 12] (shorter than any window): -1.
        assert_eq!(z[2], -1.0);
    }

    #[test]
    fn window_clamped_to_one_never_has_two_samples() {
        let z = compute_z_scores(&[1.0, 2.0, 3.0], 0);
        assert_eq!(z, vec![0.0, 0.0, 0.0]);
    }
}

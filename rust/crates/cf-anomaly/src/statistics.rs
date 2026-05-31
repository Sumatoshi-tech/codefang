//! Mean / standard-deviation helpers for anomaly detection.
//!
//! Port of `meanStd` in `internal/analyzers/anomaly/statistics.go`. Uses the
//! population standard deviation (divides by `n`, not `n-1`), matching Go.

/// Returns the arithmetic mean and the **population** standard deviation of
/// `values`. An empty slice yields `(0.0, 0.0)`, exactly like Go's `meanStd`.
///
/// The two-pass formulation (sum, then summed squared deviations) mirrors the Go
/// source operation-for-operation so the floating-point result is bit-identical.
#[must_use]
pub fn mean_std(values: &[f64]) -> (f64, f64) {
    let n = values.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let mut sum = 0.0_f64;
    for &v in values {
        sum += v;
    }
    let mean = sum / n as f64;
    let mut variance = 0.0_f64;
    for &v in values {
        variance += (v - mean) * (v - mean);
    }
    variance /= n as f64;
    (mean, variance.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_zero() {
        assert_eq!(mean_std(&[]), (0.0, 0.0));
    }

    #[test]
    fn single_value_zero_std() {
        assert_eq!(mean_std(&[5.0]), (5.0, 0.0));
    }

    #[test]
    fn population_std() {
        // values 2,4,4,4,5,5,7,9 -> mean 5, population std 2.0
        let (mean, std) = mean_std(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        assert!((mean - 5.0).abs() < 1e-12);
        assert!((std - 2.0).abs() < 1e-12);
    }
}

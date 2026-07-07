//! `cf-alg-stats` — core statistical functions for numerical analysis.
//!
//! Rust port of the Go package `pkg/alg/stats` (statistics: quantiles,
//! `MeanStdDev`, summaries, plus an exponential moving average). Used by
//! streaming and many analyzers. See `specs/rust-rewrite/DESIGN.md` §1.
//!
//! # Behavior parity
//!
//! All standard-deviation calculations use **population** stddev (÷n, not
//! ÷(n−1)), matching the Go original. Numeric routines reproduce the Go
//! algorithms operation-for-operation so that derived values flowing into
//! machine reports stay byte-identical.
//!
//! This crate produces no report output of its own (the Go package marshals
//! nothing), so it does not depend on the `cf-gojson` serialization crate — it
//! only computes scalars consumed by callers that do the serialization.
//!
//! # Module layout vs. Go
//!
//! | Go symbol | Rust equivalent |
//! | --- | --- |
//! | `Mean` | [`mean`] |
//! | `MeanStdDev` | [`mean_std_dev`] |
//! | `ToPercent` / `PercentMultiplier` | [`to_percent`] / [`PERCENT_MULTIPLIER`] |
//! | `Percentile` / `Median` | [`percentile`] / [`median`] |
//! | `Clamp` / `Min` / `Max` / `Sum` | [`clamp`] / [`min`] / [`max`] / [`sum`] |
//! | `ExceedsThreshold` | [`exceeds_threshold`] |
//! | `Distribution` | [`distribution`] |
//! | `EMA` / `NewEMA` | [`Ema`] / [`Ema::new`] |
//! | `PercentileMedian` / `PercentileP95` | [`PERCENTILE_MEDIAN`] / [`PERCENTILE_P95`] |
//! | `ZScoreMaxSentinel` | [`Z_SCORE_MAX_SENTINEL`] |

use std::collections::HashMap;

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-alg-stats";

/// The standard multiplier for converting ratios to percentages.
///
/// Ratio-to-percent multiplier (compatibility constant).
pub const PERCENT_MULTIPLIER: f64 = 100.0;

/// 50th-percentile threshold (the median).
///
/// Median percentile rank (compatibility constant).
pub const PERCENTILE_MEDIAN: f64 = 0.5;

/// 95th-percentile threshold.
///
/// 95th-percentile rank (compatibility constant).
pub const PERCENTILE_P95: f64 = 0.95;

/// The cap for z-score when stddev is zero but the value differs from the mean.
///
/// Z-score saturation sentinel (`100.0`). Provided for parity with the
/// Go package; consumers apply it.
pub const Z_SCORE_MAX_SENTINEL: f64 = 100.0;

/// Returns the arithmetic mean of `values`.
///
/// Returns `0` for an empty slice, matching the Go `Mean`.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::mean;
/// assert_eq!(mean(&[1.0, 2.0, 3.0, 4.0, 5.0]), 3.0);
/// assert_eq!(mean(&[]), 0.0);
/// ```
#[must_use]
pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sum = 0.0_f64;

    for &v in values {
        sum += v;
    }

    sum / values.len() as f64
}

/// Returns the arithmetic mean and **population** standard deviation of
/// `values` as `(mean, stddev)`.
///
/// Returns `(0, 0)` for an empty slice. Standard deviation divides by `n`
/// (population), exactly as the Go `MeanStdDev` does: it reuses [`mean`] and
/// sums squared deviations in input order before taking `sqrt(sumSq / n)`.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::mean_std_dev;
/// let (m, sd) = mean_std_dev(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
/// assert_eq!(m, 5.0);
/// assert_eq!(sd, 2.0);
/// ```
#[must_use]
pub fn mean_std_dev(values: &[f64]) -> (f64, f64) {
    let count = values.len();
    if count == 0 {
        return (0.0, 0.0);
    }

    let mean_val = mean(values);

    let mut sum_sq = 0.0_f64;

    for &v in values {
        let diff = v - mean_val;
        sum_sq += diff * diff;
    }

    (mean_val, (sum_sq / count as f64).sqrt())
}

/// Converts a ratio (0.0–1.0) to a percentage (0–100).
///
/// Converts a ratio to a percentage: `ratio * PercentMultiplier`.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::to_percent;
/// assert_eq!(to_percent(0.75), 75.0);
/// ```
#[must_use]
pub fn to_percent(ratio: f64) -> f64 {
    ratio * PERCENT_MULTIPLIER
}

/// Returns the `p`-th percentile of `values` using linear interpolation.
///
/// `p` must be in `[0, 1]`. The input slice is not modified (a copy is sorted
/// internally). Returns `0` for an empty slice.
///
/// This reproduces the Go `Percentile` exactly:
/// - sort a copy with Go-`slices.Sort` float ordering (see [`go_sort_f64`]);
/// - `idx = p * (n-1)`, `lower = floor(idx)`, `upper = ceil(idx)`;
/// - if `lower == upper` or `upper >= n`, return `sorted[lower]`;
/// - otherwise interpolate `sorted[lower]*(1-frac) + sorted[upper]*frac` where
///   `frac = idx - lower`.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::{percentile, PERCENTILE_MEDIAN};
/// assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], PERCENTILE_MEDIAN), 2.5);
/// ```
#[must_use]
pub fn percentile(values: &[f64], p: f64) -> f64 {
    let count = values.len();
    if count == 0 {
        return 0.0;
    }

    let mut sorted = values.to_vec();
    go_sort_f64(&mut sorted);

    let idx = p * (count - 1) as f64;
    // Go: lower := int(math.Floor(idx)); upper := int(math.Ceil(idx)).
    // Integer conversion truncates toward zero (report contract); floor/ceil of a value in
    // [0, count-1] are non-negative, so `as usize` matches int() here.
    let lower_f = idx.floor();
    let upper_f = idx.ceil();
    let lower = lower_f as usize;
    let upper = upper_f as usize;

    if lower == upper || upper >= count {
        return sorted[lower];
    }

    let frac = idx - lower_f;

    sorted[lower] * (1.0 - frac) + sorted[upper] * frac
}

/// Returns the 50th percentile of `values`.
///
/// Returns `0` for an empty slice.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::median;
/// assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
/// ```
#[must_use]
pub fn median(values: &[f64]) -> f64 {
    percentile(values, PERCENTILE_MEDIAN)
}

/// Restricts `val` to the range `[lo, hi]`.
///
/// Clamps to `[lo, hi]`: `max(lo, min(val, hi))`. Uses
/// [`PartialOrd`] so it accepts both integers and floats; for `f64`, callers
/// must avoid NaN bounds (the Go original likewise relies on `cmp.Ordered`
/// total order semantics and does not special-case NaN).
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::clamp;
/// assert_eq!(clamp(15.0, 0.0, 10.0), 10.0);
/// assert_eq!(clamp(15, 0, 10), 10);
/// ```
#[must_use]
pub fn clamp<T: PartialOrd>(val: T, lo: T, hi: T) -> T {
    // Go: max(lo, min(val, hi)). Reproduce with the same nesting.
    go_max(lo, go_min(val, hi))
}

/// Returns the smallest element in `values`.
///
/// Returns the [`Default`] value of `T` for an empty slice, mirroring Go's
/// "zero value of T". Comparison uses `<` (`PartialOrd`), as the Go original
/// scans linearly with `if v < result`.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::min;
/// assert_eq!(min(&[3.0, 1.0, 4.0, 1.5, 9.0]), 1.0);
/// assert_eq!(min::<f64>(&[]), 0.0);
/// ```
#[must_use]
pub fn min<T: PartialOrd + Copy + Default>(values: &[T]) -> T {
    if values.is_empty() {
        return T::default();
    }

    let mut result = values[0];

    for &v in &values[1..] {
        if v < result {
            result = v;
        }
    }

    result
}

/// Returns the largest element in `values`.
///
/// Returns the [`Default`] value of `T` for an empty slice, mirroring Go's
/// "zero value of T". Comparison uses `>` (`PartialOrd`), as the Go original
/// scans linearly with `if v > result`.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::max;
/// assert_eq!(max(&[3.0, 1.0, 9.0, 4.0]), 9.0);
/// assert_eq!(max::<i64>(&[]), 0);
/// ```
#[must_use]
pub fn max<T: PartialOrd + Copy + Default>(values: &[T]) -> T {
    if values.is_empty() {
        return T::default();
    }

    let mut result = values[0];

    for &v in &values[1..] {
        if v > result {
            result = v;
        }
    }

    result
}

/// Reports whether `observed` diverges from `predicted` by more than
/// `threshold` (a fraction, e.g. `0.1` = 10%).
///
/// Returns `false` when `predicted <= 0` (no meaningful baseline). Matches the reference
/// `ExceedsThreshold`: computes `divergence = (observed - predicted)/predicted`,
/// takes its absolute value, and returns `divergence > threshold` (strictly
/// greater — equal-to-threshold is *not* exceeded).
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::exceeds_threshold;
/// assert!(exceeds_threshold(115.0, 100.0, 0.1));
/// assert!(!exceeds_threshold(110.0, 100.0, 0.1));
/// assert!(!exceeds_threshold(50.0, 0.0, 0.1));
/// ```
#[must_use]
pub fn exceeds_threshold(observed: f64, predicted: f64, threshold: f64) -> bool {
    if predicted <= 0.0 {
        return false;
    }

    let mut divergence = (observed - predicted) / predicted;
    if divergence < 0.0 {
        divergence = -divergence;
    }

    divergence > threshold
}

/// Counts items per label as determined by `classify`.
///
/// Returns `None` for a `None` slice and an empty map for an empty slice,
/// mirroring the reference `Distribution` (which returns `nil` for a `nil` slice and an
/// empty `map` for an empty slice). The returned map's iteration order is
/// unspecified, exactly like a Go `map` — callers that serialize it must sort
/// keys (as `cf-gojson` does for map-origin objects).
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::distribution;
/// let got = distribution(Some(&[-3, 0, 5, -1, 7]), |&n| {
///     if n > 0 { "positive" } else if n < 0 { "negative" } else { "zero" }.to_string()
/// });
/// let got = got.unwrap();
/// assert_eq!(got["positive"], 2);
/// assert_eq!(got["negative"], 2);
/// assert_eq!(got["zero"], 1);
/// ```
#[must_use]
pub fn distribution<T, F>(items: Option<&[T]>, classify: F) -> Option<HashMap<String, i64>>
where
    F: Fn(&T) -> String,
{
    let items = items?;

    let mut counts: HashMap<String, i64> = HashMap::with_capacity(items.len());

    for item in items {
        *counts.entry(classify(item)).or_insert(0) += 1;
    }

    Some(counts)
}

/// Returns the sum of all elements in `values`.
///
/// Returns the [`Default`] value of `T` for an empty slice (the reference's "zero value of
/// T"). Accumulates in input order via `+=`, matching the Go `Sum`.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::sum;
/// assert_eq!(sum(&[1.0, 2.0, 3.0]), 6.0);
/// assert_eq!(sum(&[1, 2, 3, 4]), 10);
/// ```
#[must_use]
pub fn sum<T>(values: &[T]) -> T
where
    T: Copy + Default + std::ops::Add<Output = T>,
{
    let mut result = T::default();

    for &v in values {
        result = result + v;
    }

    result
}

/// An exponential moving average with a fixed smoothing factor.
///
/// Rust port of the Go `EMA` type. The first [`update`](Ema::update) call
/// initializes the average to the observation value; subsequent calls apply
/// `value = alpha*v + (1-alpha)*value` in exactly that operand order so the
/// floating-point result matches Go bit-for-bit.
///
/// # Examples
///
/// ```
/// # use cf_alg_stats::Ema;
/// let mut ema = Ema::new(0.3);
/// assert_eq!(ema.update(10.0), 10.0); // first call initializes
/// assert_eq!(ema.update(20.0), 13.0); // 0.3*20 + 0.7*10
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Ema {
    alpha: f64,
    value: f64,
    initialized: bool,
}

impl Ema {
    /// Creates an EMA with the given smoothing factor `alpha` in `(0, 1]`.
    ///
    /// The value starts at `0` and is uninitialized
    /// until the first [`update`](Ema::update).
    #[must_use]
    pub const fn new(alpha: f64) -> Self {
        Self {
            alpha,
            value: 0.0,
            initialized: false,
        }
    }

    /// Feeds a new observation and returns the updated average.
    ///
    /// The first call initializes the EMA to the observation value; later calls
    /// compute `alpha*v + (1-alpha)*value`.
    pub fn update(&mut self, v: f64) -> f64 {
        if !self.initialized {
            self.value = v;
            self.initialized = true;

            return self.value;
        }

        self.value = self.alpha * v + (1.0 - self.alpha) * self.value;

        self.value
    }

    /// Returns the current EMA value (`0` before any [`update`](Ema::update)).
    #[must_use]
    pub fn value(&self) -> f64 {
        self.value
    }

    /// Reports whether [`update`](Ema::update) has been called at least once.
    #[must_use]
    pub fn initialized(&self) -> bool {
        self.initialized
    }
}

/// `min(a, b)` with the reference tie-break for ordered types.
///
/// Returns `a` when `a <= b`, else `b`. For the parity-relevant
/// usage (clamp with finite bounds) this is equivalent; NaN handling is not
/// special-cased, consistent with the Go original relying on `cmp.Ordered`.
#[inline]
fn go_min<T: PartialOrd>(a: T, b: T) -> T {
    if a < b {
        a
    } else {
        b
    }
}

/// `max(a, b)` with the reference tie-break for ordered types.
#[inline]
fn go_max<T: PartialOrd>(a: T, b: T) -> T {
    if a > b {
        a
    } else {
        b
    }
}

/// Sorts `values` ascending using the reference float ordering (NaN-first total order).
///
/// The reference sort on floats uses a three-way comparator that orders NaN as
/// **less than** every other value (NaNs sort to the front) and treats `-0.0`
/// and `+0.0` as equal. [`f64::total_cmp`] differs (it places NaN at the
/// extremes and distinguishes signed zero), so we implement that comparator
/// directly to keep percentile results identical on NaN/zero edge cases.
fn go_sort_f64(values: &mut [f64]) {
    values.sort_by(go_cmp_f64);
}

/// Three-way float comparison (reference `cmp.Compare[float64]` semantics).
///
/// `x < y` ⇒ -1, `x > y` ⇒ +1, otherwise 0, with the
/// special rule that a NaN operand is considered less than a non-NaN operand
/// (and two NaNs compare equal). This makes NaNs sort to the front, exactly as
/// `slices.Sort` does.
fn go_cmp_f64(a: &f64, b: &f64) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    if a < b {
        return Ordering::Less;
    }
    if a > b {
        return Ordering::Greater;
    }
    // Neither <, > — either equal, or NaN involved. Go: isNaN(x) && !isNaN(y)
    // => -1; !isNaN(x) && isNaN(y) => +1; both NaN or both equal => 0.
    let a_nan = a.is_nan();
    let b_nan = b.is_nan();
    match (a_nan, b_nan) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clamp, distribution, exceeds_threshold, go_sort_f64, max, mean, mean_std_dev, median, min,
        percentile, sum, to_percent, Ema, PERCENTILE_MEDIAN, PERCENTILE_P95, PERCENT_MULTIPLIER,
    };
    use std::collections::HashMap;

    const EPS: f64 = 0.0001;

    fn assert_in_delta(expected: f64, got: f64, delta: f64) {
        assert!(
            (expected - got).abs() <= delta,
            "expected {expected} got {got} (delta {delta})"
        );
    }

    // makeSequence returns [1.0, 2.0, ..., n], mirroring the Go test helper.
    fn make_sequence(n: usize) -> Vec<f64> {
        (0..n).map(|i| (i + 1) as f64).collect()
    }

    // --- stats_test.go ---

    #[test]
    fn test_clamp() {
        // (val, lo, hi, expected)
        let cases = [
            (5.0, 0.0, 10.0, 5.0),   // within_range
            (-1.0, 0.0, 10.0, 0.0),  // below_min
            (15.0, 0.0, 10.0, 10.0), // above_max
            (0.0, 0.0, 10.0, 0.0),   // at_min
            (10.0, 0.0, 10.0, 10.0), // at_max
        ];
        for (val, lo, hi, expected) in cases {
            assert_in_delta(expected, clamp(val, lo, hi), EPS);
        }
    }

    #[test]
    fn test_clamp_int() {
        assert_eq!(clamp(15, 0, 10), 10);
    }

    #[test]
    fn test_min() {
        assert_in_delta(0.0, min::<f64>(&[]), EPS);
        assert_in_delta(7.0, min(&[7.0]), EPS);
        assert_in_delta(1.0, min(&[3.0, 1.0, 4.0, 1.5, 9.0]), EPS);
    }

    #[test]
    fn test_max() {
        assert_in_delta(0.0, max::<f64>(&[]), EPS);
        assert_in_delta(9.0, max(&[3.0, 1.0, 9.0, 4.0]), EPS);
    }

    #[test]
    fn test_sum() {
        assert_in_delta(0.0, sum::<f64>(&[]), EPS);
        assert_in_delta(6.0, sum(&[1.0, 2.0, 3.0]), EPS);
        assert_eq!(sum(&[1, 2, 3, 4]), 10);
    }

    #[test]
    fn test_min_int() {
        assert_eq!(min(&[3, 1, 4, 1, 5]), 1);
    }

    #[test]
    fn test_max_int() {
        assert_eq!(max(&[3, 1, 4, 1, 5]), 5);
    }

    #[test]
    fn test_percentile() {
        // (input, p, expected)
        let empty: Vec<f64> = Vec::new();
        let seq = make_sequence(100);
        let cases: &[(&[f64], f64, f64)] = &[
            (&empty, PERCENTILE_MEDIAN, 0.0),           // empty_returns_zero
            (&[7.0], PERCENTILE_MEDIAN, 7.0),           // single_element
            (&[3.0, 1.0, 2.0], PERCENTILE_MEDIAN, 2.0), // median_odd
            (&[1.0, 2.0, 3.0, 4.0], PERCENTILE_MEDIAN, 2.5), // median_even
            (&seq, PERCENTILE_P95, 95.05),              // p95_of_100
            (&[5.0, 1.0, 9.0], 0.0, 1.0),               // p0_is_min
            (&[5.0, 1.0, 9.0], 1.0, 9.0),               // p100_is_max
            (&[9.0, 1.0, 5.0, 3.0, 7.0], PERCENTILE_MEDIAN, 5.0), // unsorted_input
        ];
        for (input, p, expected) in cases {
            assert_in_delta(*expected, percentile(input, *p), 0.1);
        }
    }

    #[test]
    fn test_median() {
        assert_in_delta(2.0, median(&[3.0, 1.0, 2.0]), EPS);
    }

    #[test]
    fn test_mean_std_dev() {
        // (input, want_mean, want_stddev)
        let empty: Vec<f64> = Vec::new();
        let cases: &[(&[f64], f64, f64)] = &[
            (&empty, 0.0, 0.0),                                    // empty_returns_zeros
            (&[5.0], 5.0, 0.0),                                    // single_element_zero_stddev
            (&[3.0, 3.0, 3.0], 3.0, 0.0),                          // uniform_values_zero_stddev
            (&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0], 5.0, 2.0), // known_population_stddev
        ];
        for (input, want_mean, want_stddev) in cases {
            let (m, sd) = mean_std_dev(input);
            assert_in_delta(*want_mean, m, EPS);
            assert_in_delta(*want_stddev, sd, EPS);
        }
    }

    #[test]
    fn test_to_percent() {
        let cases = [
            (0.75, 75.0),              // positive_ratio
            (0.0, 0.0),                // zero
            (-0.25, -25.0),            // negative_ratio
            (1.0, PERCENT_MULTIPLIER), // full_ratio
            (1.5, 150.0),              // above_one
        ];
        for (ratio, expected) in cases {
            assert_in_delta(expected, to_percent(ratio), EPS);
        }
    }

    #[test]
    fn test_percent_multiplier_constant() {
        assert_in_delta(100.0, PERCENT_MULTIPLIER, EPS);
    }

    #[test]
    fn test_mean() {
        let empty: Vec<f64> = Vec::new();
        let cases: &[(&[f64], f64)] = &[
            (&empty, 0.0),                     // empty_returns_zero
            (&[5.0], 5.0),                     // single_element
            (&[2.0, 4.0], 3.0),                // two_elements
            (&[1.0, 2.0, 3.0, 4.0, 5.0], 3.0), // known_mean
            (&[-2.0, -4.0], -3.0),             // negative_values
        ];
        for (input, expected) in cases {
            assert_in_delta(*expected, mean(input), EPS);
        }
    }

    #[test]
    fn test_exceeds_threshold() {
        // (observed, predicted, threshold, want)
        let cases = [
            (105.0, 100.0, 0.1, false), // below_threshold
            (115.0, 100.0, 0.1, true),  // above_threshold
            (110.0, 100.0, 0.1, false), // exact_threshold_not_exceeded
            (85.0, 100.0, 0.1, true),   // negative_divergence_above
            (95.0, 100.0, 0.1, false),  // negative_divergence_below
            (50.0, 0.0, 0.1, false),    // zero_predicted
            (50.0, -10.0, 0.1, false),  // negative_predicted
            (-120.0, 100.0, 0.5, true), // negative_observed
            (100.0, 100.0, 0.1, false), // both_equal
            (100.0, 100.0, 0.0, false), // zero_threshold_equal
            (100.1, 100.0, 0.0, true),  // zero_threshold_any_diff
        ];
        for (observed, predicted, threshold, want) in cases {
            assert_eq!(
                want,
                exceeds_threshold(observed, predicted, threshold),
                "observed={observed} predicted={predicted} threshold={threshold}"
            );
        }
    }

    fn classify_sign(n: &i64) -> String {
        if *n > 0 {
            "positive".to_string()
        } else if *n < 0 {
            "negative".to_string()
        } else {
            "zero".to_string()
        }
    }

    #[test]
    fn test_distribution_nil_returns_nil() {
        let got = distribution::<i64, _>(None, classify_sign);
        assert!(got.is_none());
    }

    #[test]
    fn test_distribution_empty_returns_empty_map() {
        let got = distribution(Some(&[] as &[i64]), classify_sign);
        let got = got.expect("empty slice yields Some(empty map)");
        assert!(got.is_empty());
    }

    #[test]
    fn test_distribution_single_item() {
        let got = distribution(Some(&[5_i64]), classify_sign).unwrap();
        let mut expected = HashMap::new();
        expected.insert("positive".to_string(), 1);
        assert_eq!(got, expected);
    }

    #[test]
    fn test_distribution_multiple_buckets() {
        let got = distribution(Some(&[-3_i64, 0, 5, -1, 7]), classify_sign).unwrap();
        let mut expected = HashMap::new();
        expected.insert("negative".to_string(), 2);
        expected.insert("zero".to_string(), 1);
        expected.insert("positive".to_string(), 2);
        assert_eq!(got, expected);
    }

    #[test]
    fn test_distribution_all_same_bucket() {
        let got = distribution(Some(&[1_i64, 2, 3]), classify_sign).unwrap();
        let mut expected = HashMap::new();
        expected.insert("positive".to_string(), 3);
        assert_eq!(got, expected);
    }

    #[test]
    fn test_distribution_string_items() {
        let classify_len = |s: &&str| {
            if s.len() <= 3 {
                "short".to_string()
            } else {
                "long".to_string()
            }
        };
        let got = distribution(Some(&["ab", "hello", "cd", "world"]), classify_len).unwrap();
        let mut expected = HashMap::new();
        expected.insert("short".to_string(), 2);
        expected.insert("long".to_string(), 2);
        assert_eq!(got, expected);
    }

    // --- ema_test.go ---

    #[test]
    fn test_new_ema() {
        let ema = Ema::new(0.3);
        assert_in_delta(0.0, ema.value(), EPS);
    }

    #[test]
    fn test_ema_first_update_initializes() {
        let mut ema = Ema::new(0.3);
        let got = ema.update(10.0);
        assert_in_delta(10.0, got, EPS);
        assert_in_delta(10.0, ema.value(), EPS);
    }

    #[test]
    fn test_ema_subsequent_updates_smooth() {
        let mut ema = Ema::new(0.3);
        ema.update(10.0); // Initialize to 10.
                          // Second update: 0.3 * 20 + 0.7 * 10 = 6 + 7 = 13.
        let got = ema.update(20.0);
        assert_in_delta(13.0, got, EPS);
    }

    #[test]
    fn test_ema_alpha_one_tracks_exactly() {
        let mut ema = Ema::new(1.0);
        ema.update(10.0);
        let got = ema.update(20.0);
        // alpha=1: 1.0 * 20 + 0.0 * 10 = 20.
        assert_in_delta(20.0, got, EPS);
    }

    #[test]
    fn test_ema_initialized() {
        let mut ema = Ema::new(0.3);
        assert!(!ema.initialized());
        ema.update(10.0);
        assert!(ema.initialized());
    }

    #[test]
    fn test_ema_convergence() {
        let mut ema = Ema::new(0.3);
        for _ in 0..100 {
            ema.update(50.0);
        }
        // After many updates of the same value, EMA should converge to it.
        assert_in_delta(50.0, ema.value(), EPS);
    }

    // --- additional parity checks for edge cases the Go tests imply ---

    #[test]
    fn test_go_sort_f64_nan_sorts_first() {
        // The reference float order places NaN at the front; verify our comparator does too.
        let mut v = vec![3.0, f64::NAN, 1.0, 2.0];
        go_sort_f64(&mut v);
        assert!(v[0].is_nan());
        assert_eq!(&v[1..], &[1.0, 2.0, 3.0]);
    }
}

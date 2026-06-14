//! Halstead derived-metric calculation.
//!
//! Takes the four raw Halstead counts ([`HalsteadCounts`]) and returns the
//! eight derived measures ([`DerivedMetrics`]), which both the file-level and
//! per-function aggregate views then store.

/// Standard constant used in time-to-program estimation (18 seconds).
pub const TIME_CONSTANT: f64 = 18.0;

/// Standard constant used in delivered-bugs estimation (`B = V / 3000`).
pub const BUG_CONSTANT: f64 = 3000.0;

/// Divisor in the difficulty formula `D = (n1 / 2) · (N2 / n2)`.
pub const DIFFICULTY_DIVISOR: f64 = 2.0;

/// The four raw Halstead counts that feed the derived measures.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HalsteadCounts {
    /// n1 — distinct operators.
    pub distinct_operators: i64,
    /// n2 — distinct operands.
    pub distinct_operands: i64,
    /// N1 — total operators.
    pub total_operators: i64,
    /// N2 — total operands.
    pub total_operands: i64,
}

/// The eight derived Halstead measures.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DerivedMetrics {
    /// n — vocabulary = n1 + n2.
    pub vocabulary: i64,
    /// N — length = N1 + N2.
    pub length: i64,
    /// Estimated length = n1·log2(n1) + n2·log2(n2) (0 when either is 0).
    pub estimated_length: f64,
    /// V — volume = N · log2(n) (0 when n == 0).
    pub volume: f64,
    /// D — difficulty = (n1/2)·(N2/n2) (0 when n2 == 0).
    pub difficulty: f64,
    /// E — effort = V · D.
    pub effort: f64,
    /// T — time to program = E / 18.
    pub time_to_program: f64,
    /// B — delivered bugs = V / 3000.
    pub delivered_bugs: f64,
}

/// Stateless calculator of Halstead derived metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsCalculator;

impl MetricsCalculator {
    /// Creates a new calculator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Sums all values in an integer count map.
    #[must_use]
    pub fn sum_map<S: ::std::hash::BuildHasher>(
        &self,
        m: &std::collections::HashMap<String, i64, S>,
    ) -> i64 {
        m.values().sum()
    }

    /// Calculates all derived Halstead metrics from the four raw counts.
    ///
    /// The zero-guards are part of the report contract: estimated length is 0
    /// unless both distinct counts are positive; volume is 0 when vocabulary
    /// is 0; difficulty is 0 when distinct operands is 0.
    ///
    /// Vocabulary (`n1 + n2`) and length (`N1 + N2`) are exact sums:
    ///
    /// ```
    /// use cf_halstead::calculator::{HalsteadCounts, MetricsCalculator};
    ///
    /// let d = MetricsCalculator::new().calculate(HalsteadCounts {
    ///     distinct_operators: 3,
    ///     distinct_operands: 4,
    ///     total_operators: 6,
    ///     total_operands: 8,
    /// });
    /// assert_eq!(d.vocabulary, 7); // 3 + 4
    /// assert_eq!(d.length, 14);    // 6 + 8
    /// assert!(d.volume > 0.0);
    /// assert!(d.difficulty > 0.0);
    /// ```
    ///
    /// All-zero counts produce all-zero measures (no division by zero):
    ///
    /// ```
    /// use cf_halstead::calculator::{HalsteadCounts, MetricsCalculator};
    ///
    /// let d = MetricsCalculator::new().calculate(HalsteadCounts::default());
    /// assert_eq!(d.volume, 0.0);
    /// assert_eq!(d.difficulty, 0.0);
    /// assert_eq!(d.estimated_length, 0.0);
    /// ```
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // token counts are far below 2^52
    pub fn calculate(&self, counts: HalsteadCounts) -> DerivedMetrics {
        let distinct_ops = counts.distinct_operators;
        let distinct_opnds = counts.distinct_operands;
        let total_ops = counts.total_operators;
        let total_opnds = counts.total_operands;

        let vocabulary = distinct_ops + distinct_opnds;
        let length = total_ops + total_opnds;

        let estimated_length = if distinct_ops > 0 && distinct_opnds > 0 {
            (distinct_ops as f64) * go_log2(distinct_ops as f64)
                + (distinct_opnds as f64) * go_log2(distinct_opnds as f64)
        } else {
            0.0
        };

        let volume = if vocabulary > 0 {
            (length as f64) * go_log2(vocabulary as f64)
        } else {
            0.0
        };

        let difficulty = if distinct_opnds > 0 {
            (distinct_ops as f64 / DIFFICULTY_DIVISOR) * (total_opnds as f64 / distinct_opnds as f64)
        } else {
            0.0
        };

        let effort = volume * difficulty;
        let time_to_program = effort / TIME_CONSTANT;
        let delivered_bugs = volume / BUG_CONSTANT;

        DerivedMetrics {
            vocabulary,
            length,
            estimated_length,
            volume,
            difficulty,
            effort,
            time_to_program,
            delivered_bugs,
        }
    }
}

// --- Bit-exact base-2 logarithm (reference-implementation port) -------------
//
// The platform libm `f64::log2` differs from the reference implementation's
// Log2 in the last ULP for some inputs (e.g. n=3), which shifts derived
// Halstead floats (volume/effort) and any downstream percentile that
// interpolates near those values. The reference computes
// `Log2(x) = log(frac)·(1/ln2) + exp` via Frexp, with its own polynomial log;
// this path is ported exactly so the volumes are byte-identical in reports
// (pinned by the differential gate). The algorithm and coefficients below are
// FROZEN.

use std::f64::consts::{LN_2, SQRT_2};

/// Frexp (normal-positive path): `(frac, exp)` with `frac ∈ [0.5, 1)` and
/// `x == frac · 2^exp`.
fn go_frexp(f: f64) -> (f64, i32) {
    if f == 0.0 || !f.is_finite() {
        return (f, 0);
    }
    const SMALLEST_NORMAL: f64 = 2.225_073_858_507_201_4e-308; // 2**-1022
    let (x, mut exp) = if f.abs() < SMALLEST_NORMAL {
        (f * (1u64 << 52) as f64, -52)
    } else {
        (f, 0)
    };
    let bits = x.to_bits();
    const SHIFT: u64 = 52;
    const MASK: u64 = 0x7FF;
    const BIAS: i64 = 1023;
    exp += (((bits >> SHIFT) & MASK) as i64 - BIAS + 1) as i32;
    let mut nb = bits;
    nb &= !(MASK << SHIFT);
    nb |= ((-1 + BIAS) as u64) << SHIFT;
    (f64::from_bits(nb), exp)
}

/// Natural log via the pinned polynomial approximation.
///
/// The coefficient literals keep their full published precision on purpose;
/// they round to the exact f64 bit patterns the reference uses.
#[allow(clippy::excessive_precision)]
fn go_log(x: f64) -> f64 {
    const LN2HI: f64 = 6.931_471_803_691_238_164_9e-01;
    const LN2LO: f64 = 1.908_214_929_270_587_700_02e-10;
    const L1: f64 = 6.666_666_666_666_735_130e-01;
    const L2: f64 = 3.999_999_999_940_941_908e-01;
    const L3: f64 = 2.857_142_874_366_239_149e-01;
    const L4: f64 = 2.222_219_843_214_978_396e-01;
    const L5: f64 = 1.818_357_216_161_805_012e-01;
    const L6: f64 = 1.531_383_769_920_937_332e-01;
    const L7: f64 = 1.479_819_860_511_658_591e-01;

    if x.is_nan() || x == f64::INFINITY {
        return x;
    }
    if x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }

    let (mut f1, mut ki) = go_frexp(x);
    if f1 < SQRT_2 / 2.0 {
        f1 *= 2.0;
        ki -= 1;
    }
    let f = f1 - 1.0;
    let k = f64::from(ki);

    let s = f / (2.0 + f);
    let s2 = s * s;
    let s4 = s2 * s2;
    let t1 = s2 * (L1 + s4 * (L3 + s4 * (L5 + s4 * L7)));
    let t2 = s4 * (L2 + s4 * (L4 + s4 * L6));
    let r = t1 + t2;
    let hfsq = 0.5 * f * f;
    k * LN2HI - ((hfsq - (s * (hfsq + r) + k * LN2LO)) - f)
}

/// Bit-exact base-2 logarithm (Frexp + pinned polynomial log).
///
/// Exact powers of two return exact integers; non-powers match the reference
/// implementation bit-for-bit (which can differ from the platform
/// [`f64::log2`] in the last ULP, e.g. for `3.0`):
///
/// ```
/// use cf_halstead::calculator::go_log2;
/// assert_eq!(go_log2(8.0), 3.0);
/// assert_eq!(go_log2(3.0).to_bits(), 0x3ff9_5c01_a39f_bd69);
/// ```
#[must_use]
pub fn go_log2(x: f64) -> f64 {
    let (frac, exp) = go_frexp(x);
    if frac == 0.5 {
        return f64::from(exp - 1);
    }
    go_log(frac) * (1.0 / LN_2) + f64::from(exp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vocabulary and length are exact sums; the derived measures are
    /// positive.
    #[test]
    fn calculate_metrics_known_values() {
        let d = MetricsCalculator::new().calculate(HalsteadCounts {
            distinct_operators: 3,
            distinct_operands: 4,
            total_operators: 6,
            total_operands: 8,
        });
        assert_eq!(d.vocabulary, 7);
        assert_eq!(d.length, 14);
        assert!(d.volume > 0.0);
        assert!(d.difficulty > 0.0);
        assert!(d.effort > 0.0);
    }

    #[test]
    fn delivered_bugs_uses_volume() {
        let d = MetricsCalculator::new().calculate(HalsteadCounts {
            distinct_operators: 2,
            distinct_operands: 3,
            total_operators: 4,
            total_operands: 6,
        });
        let expected = d.volume / BUG_CONSTANT;
        assert!((expected - d.delivered_bugs).abs() < 1e-12);
    }

    #[test]
    fn zero_counts_are_zero() {
        let d = MetricsCalculator::new().calculate(HalsteadCounts::default());
        assert_eq!(d.vocabulary, 0);
        assert_eq!(d.length, 0);
        assert_eq!(d.estimated_length, 0.0);
        assert_eq!(d.volume, 0.0);
        assert_eq!(d.difficulty, 0.0);
        assert_eq!(d.effort, 0.0);
        assert_eq!(d.delivered_bugs, 0.0);
    }

    /// Bit-level regression pin for the ported log2 path: these are the exact
    /// outputs the report contract depends on (the platform `f64::log2`
    /// differs in the last ULP for some of these, e.g. 3.0).
    #[test]
    fn go_log2_bit_pins() {
        assert_eq!(go_log2(3.0).to_bits(), 0x3ff9_5c01_a39f_bd69);
        assert_eq!(go_log2(5.0).to_bits(), 0x4002_934f_0979_a371);
        assert_eq!(go_log2(7.0).to_bits(), 0x4006_7576_7f54_042d);
        assert_eq!(go_log2(10.0).to_bits(), 0x400a_934f_0979_a371);
        assert_eq!(go_log2(1000.0).to_bits(), 0x4023_ee7b_471b_3a95);
        assert_eq!(go_log2(8.0), 3.0);
    }
}

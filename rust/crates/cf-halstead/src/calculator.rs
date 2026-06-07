//! Halstead derived-metric calculation (`metrics.go`).
//!
//! The Go code expresses the calculation against a set of getter/setter
//! interfaces so the same routine drives both the file-level `Metrics` and the
//! per-function `FunctionHalsteadMetrics`. In Rust we model the four inputs as a
//! small [`HalsteadCounts`] value and return the derived measures, which both
//! aggregate types then store. This is behaviorally identical and avoids the
//! interface indirection.

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

/// Stateless calculator of Halstead derived metrics (`MetricsCalculator`).
#[derive(Debug, Clone, Copy, Default)]
pub struct MetricsCalculator;

impl MetricsCalculator {
    /// Creates a new calculator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Sums all values in an integer map (`SumMap`).
    #[must_use]
    pub fn sum_map<S: ::std::hash::BuildHasher>(
        &self,
        m: &std::collections::HashMap<String, i64, S>,
    ) -> i64 {
        m.values().sum()
    }

    /// Calculates all derived Halstead metrics from the four raw counts.
    ///
    /// Reproduces `CalculateHalsteadMetrics` exactly, including the zero-guards:
    /// estimated length is 0 unless both distinct counts are positive; volume is
    /// 0 when vocabulary is 0; difficulty is 0 when distinct operands is 0.
    #[must_use]
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

// --- Bit-exact Go `math.Log2` ----------------------------------------------
//
// Rust's libm `f64::log2` differs from Go's `math.Log2` in the last ULP for some
// inputs (e.g. n=3), which shifts derived Halstead floats (volume/effort) and
// any downstream percentile that interpolates near those values. Go computes
// `Log2(x) = log(frac)·(1/Ln2) + exp` via `Frexp`, with its own polynomial
// `log` (amd64 `archLog`); we port that path exactly so the volumes are
// byte-identical to Go.

const GO_LN2: f64 = 0.693_147_180_559_945_309_417_232_121_458_176_568_075_5;
const GO_SQRT2: f64 = 1.414_213_562_373_095_048_801_688_724_209_698_078_569_7;

/// Go `math.Frexp` (normal-positive path): `(frac, exp)` with `frac ∈ [0.5, 1)`
/// and `x == frac · 2^exp`.
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

/// Go `math.log` (pure-Go polynomial, matching amd64 `archLog`).
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
    if f1 < GO_SQRT2 / 2.0 {
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

/// Go `math.Log2` (amd64: `log2` with `Frexp` + `archLog`).
#[must_use]
pub fn go_log2(x: f64) -> f64 {
    let (frac, exp) = go_frexp(x);
    if frac == 0.5 {
        return f64::from(exp - 1);
    }
    go_log(frac) * (1.0 / GO_LN2) + f64::from(exp)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `TestAnalyzer_CalculateMetrics`: vocabulary and length are
    /// exact sums; the derived measures are positive.
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

    /// Ported from `TestMetricsCalculator_DeliveredBugsUsesVolume`.
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
}

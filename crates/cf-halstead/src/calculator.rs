//! Halstead derived-metric calculation.
//!
//! Takes the four raw Halstead counts ([`HalsteadCounts`]) and returns the
//! eight derived measures ([`DerivedMetrics`]) — the same arithmetic for one
//! function, one file, and the whole codebase.

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

/// Derives the eight Halstead measures from the four raw counts.
///
/// Zero guards: estimated length is 0 unless both distinct counts are
/// positive; volume is 0 when the vocabulary is 0; difficulty is 0 when there
/// are no distinct operands.
///
/// ```
/// use cf_halstead::calculator::{derive, HalsteadCounts};
///
/// // c := a + b; return c * 2  →  n1 = 4, n2 = 4, N1 = 4, N2 = 5
/// let d = derive(HalsteadCounts {
///     distinct_operators: 4,
///     distinct_operands: 4,
///     total_operators: 4,
///     total_operands: 5,
/// });
/// assert_eq!(d.vocabulary, 8);
/// assert_eq!(d.length, 9);
/// assert_eq!(d.volume, 27.0);      // 9 · log2(8)
/// assert_eq!(d.difficulty, 2.5);   // (4/2) · (5/4)
/// assert_eq!(d.effort, 67.5);
/// assert_eq!(d.time_to_program, 3.75);
/// assert_eq!(d.delivered_bugs, 0.009);
/// ```
#[must_use]
#[allow(clippy::cast_precision_loss)] // token counts are far below 2^52
pub fn derive(counts: HalsteadCounts) -> DerivedMetrics {
    let n1 = counts.distinct_operators as f64;
    let n2 = counts.distinct_operands as f64;
    let big_n2 = counts.total_operands as f64;

    let vocabulary = counts.distinct_operators + counts.distinct_operands;
    let length = counts.total_operators + counts.total_operands;

    let estimated_length = if n1 > 0.0 && n2 > 0.0 {
        n1 * n1.log2() + n2 * n2.log2()
    } else {
        0.0
    };
    let volume = if vocabulary > 0 {
        (length as f64) * (vocabulary as f64).log2()
    } else {
        0.0
    };
    let difficulty = if n2 > 0.0 {
        (n1 / DIFFICULTY_DIVISOR) * (big_n2 / n2)
    } else {
        0.0
    };
    let effort = volume * difficulty;

    DerivedMetrics {
        vocabulary,
        length,
        estimated_length,
        volume,
        difficulty,
        effort,
        time_to_program: effort / TIME_CONSTANT,
        delivered_bugs: volume / BUG_CONSTANT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        let d = derive(HalsteadCounts {
            distinct_operators: 3,
            distinct_operands: 4,
            total_operators: 6,
            total_operands: 8,
        });
        assert_eq!(d.vocabulary, 7);
        assert_eq!(d.length, 14);
        assert!((d.volume - 14.0 * 7f64.log2()).abs() < 1e-12);
        assert!((d.difficulty - 3.0).abs() < 1e-12);
        assert!((d.effort - d.volume * 3.0).abs() < 1e-9);
        assert!((d.estimated_length - (3.0 * 3f64.log2() + 4.0 * 4f64.log2())).abs() < 1e-12);
    }

    #[test]
    fn zero_counts_are_zero() {
        assert_eq!(derive(HalsteadCounts::default()), DerivedMetrics::default());
    }

    #[test]
    fn operators_only_has_volume_but_no_difficulty() {
        let d = derive(HalsteadCounts {
            distinct_operators: 2,
            distinct_operands: 0,
            total_operators: 3,
            total_operands: 0,
        });
        assert_eq!(d.volume, 3.0);
        assert_eq!(d.difficulty, 0.0);
        assert_eq!(d.estimated_length, 0.0);
    }
}

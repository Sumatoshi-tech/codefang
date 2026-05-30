//! Scalar formatting helpers for human-readable report fields.
//!
//! Port of the formatting half of
//! `internal/analyzers/common/reportutil/reportutil.go`. These produce strings
//! that may appear in textual report fields.
//!
//! Float formatting (`format_float`, `format_percent`) reproduces Go's
//! `fmt.Sprintf("%.1f", v)`: fixed one-decimal-place rendering with IEEE-correct
//! round-half-to-even on the exact binary value. Rust's `core::fmt` float
//! formatter and Go's `strconv.FormatFloat` both implement that same rounding,
//! so the rendered digits match (e.g. `0.85 → "0.8"`, `0.05 → "0.1"`,
//! `0.125 → "0.1"`).

/// Percentage multiplier. Mirrors `PercentMultiplier` (reportutil.go:13).
pub const PERCENT_MULTIPLIER: f64 = 100.0;

/// Formats an `i64` as a base-10 string.
///
/// Mirrors `FormatInt` / `strconv.Itoa` (reportutil.go:117). Go's `int` is
/// 64-bit on the supported targets, so this port takes `i64`.
#[must_use]
pub fn format_int(v: i64) -> String {
    v.to_string()
}

/// Formats an `f64` with one decimal place.
///
/// Mirrors `FormatFloat` / `fmt.Sprintf("%.1f", v)` (reportutil.go:122).
///
/// # Examples
///
/// ```
/// use cf_reportutil::format::format_float;
/// assert_eq!(format_float(3.14159), "3.1");
/// ```
#[must_use]
pub fn format_float(v: f64) -> String {
    format!("{v:.1}")
}

/// Formats an `f64` in `[0, 1]` as a percentage string with one decimal place.
///
/// The value is scaled by [`PERCENT_MULTIPLIER`] *before* formatting, matching
/// Go's `fmt.Sprintf("%.1f%%", v*PercentMultiplier)` (reportutil.go:127) — the
/// multiply happens in `f64`, then the product is rendered.
///
/// # Examples
///
/// ```
/// use cf_reportutil::format::format_percent;
/// assert_eq!(format_percent(0.85), "85.0%");
/// ```
#[must_use]
pub fn format_percent(v: f64) -> String {
    format!("{:.1}%", v * PERCENT_MULTIPLIER)
}

/// Computes `count / total` as an `f64` in `[0, 1]`, guarding division by zero.
///
/// Returns `0.0` when `total == 0`. Mirrors `Pct` (reportutil.go:132). The
/// division is performed in `f64`, matching Go's `float64(count)/float64(total)`.
#[must_use]
pub fn pct(count: i64, total: i64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    count as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    // TestFormatInt (reportutil_test.go:173).
    #[test]
    fn format_int_basic() {
        assert_eq!(format_int(42), "42");
    }

    // TestFormatFloat (reportutil_test.go:181).
    #[test]
    fn format_float_basic() {
        assert_eq!(format_float(3.14159), "3.1");
    }

    // TestFormatPercent (reportutil_test.go:189).
    #[test]
    fn format_percent_basic() {
        assert_eq!(format_percent(0.85), "85.0%");
    }

    // TestPct_Normal (reportutil_test.go:197).
    #[test]
    fn pct_normal() {
        assert_eq!(pct(3, 10), 0.3);
    }

    // TestPct_Zero (reportutil_test.go:205).
    #[test]
    fn pct_zero() {
        assert_eq!(pct(0, 0), 0.0);
    }

    // Round-half-to-even parity with Go's %.1f, verified against the Go binary:
    // 0.85→0.8, 0.05→0.1, 0.125→0.1, 0.35→0.3, 0.25→0.2.
    #[test]
    fn format_float_round_half_to_even_matches_go() {
        assert_eq!(format_float(0.85), "0.8");
        assert_eq!(format_float(0.05), "0.1");
        assert_eq!(format_float(0.125), "0.1");
        assert_eq!(format_float(0.35), "0.3");
        assert_eq!(format_float(0.25), "0.2");
    }

    // Percent path verified against the Go binary: 0.005→0.5%, 0.125→12.5%.
    #[test]
    fn format_percent_matches_go() {
        assert_eq!(format_percent(0.005), "0.5%");
        assert_eq!(format_percent(0.125), "12.5%");
        assert_eq!(format_percent(0.0), "0.0%");
    }
}

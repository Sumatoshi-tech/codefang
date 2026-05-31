//! Go-compatible `f64` formatting.
//!
//! Two renderings are provided, both built on the **shortest round-trip** digit
//! sequence (the unique decimal that parses back to the same `f64`):
//!
//! * [`format_json_float`] reproduces Go's `encoding/json` float encoder
//!   (`encoding/json/encode.go`, `floatEncoder.encode`). This is what
//!   [`crate::marshal`] uses for every [`crate::GoValue::Float`].
//! * [`format_float_g`] reproduces `strconv.FormatFloat(f, 'g', -1, 64)`.
//!
//! They share the same digits and differ only in layout:
//!
//! | aspect | `encoding/json` | `strconv 'g'` |
//! | --- | --- | --- |
//! | exponent threshold | `abs < 1e-6 \|\| abs >= 1e21` | `decExp < -4 \|\| decExp >= 21` |
//! | exponent digits | **≥1** (`1e-5`, `1e+21`) | **≥2** (`1e-05`, `1e+21`) |
//! | sign on exponent | always | always |
//!
//! # Where the digits come from
//!
//! Rust's own `{}`/`Display` for `f64` emits the shortest round-trip decimal
//! (Grisu/Ryū), the *same digit sequence* Go's `strconv` produces. We parse that
//! Rust output back into `(sign, digits, decimal_exponent)` and re-render with
//! Go's layout rules, so the byte differences between Rust `Display` and Go are
//! eliminated while reusing the (already correct) shortest digits. The oracle
//! tests in `tests/` assert byte-equality against Go for millions-scale corpora.

/// A parsed shortest-decimal: `(-1)^neg * 0.<digits> * 10^(dec_point)`.
///
/// `digits` holds significant digits with no leading/trailing zeros (except the
/// single digit `"0"` for a zero value). `dec_point` is the position of the
/// decimal point relative to the first digit, i.e. the value equals
/// `digits * 10^(dec_point - len(digits))` — matching Go's `decimalSlice`
/// convention where `d.dp` is the number of digits before the decimal point.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shortest {
    neg: bool,
    digits: Vec<u8>, // ASCII '0'..='9', no leading zeros, no trailing zeros
    dec_point: i32,  // count of digits to the left of the decimal point
}

/// Decomposes an `f64` into its shortest-round-trip decimal via Rust's Display.
///
/// `f` must be finite (callers guarantee this; Go errors on NaN/Inf in JSON).
fn shortest_decimal(f: f64) -> Shortest {
    let neg = f.is_sign_negative();
    let abs = f.abs();

    if abs == 0.0 {
        return Shortest {
            neg,
            digits: vec![b'0'],
            dec_point: 1,
        };
    }

    // Rust's `{:e}` gives shortest mantissa + exponent: e.g. "1.234e2",
    // "5e-1", "1e21". The mantissa digits are the shortest round-trip set.
    let s = format!("{abs:e}"); // always sign-less mantissa, lowercase 'e'
    // Split "<mantissa>e<exp>".
    let (mantissa, exp_str) = s.split_once('e').expect("{:e} always has an 'e'");
    let exp: i32 = exp_str.parse().expect("valid exponent");

    // mantissa is "D" or "D.DDD" (single leading digit, then optional frac).
    let mut digits: Vec<u8> = Vec::with_capacity(mantissa.len());
    for b in mantissa.bytes() {
        if b != b'.' {
            digits.push(b);
        }
    }
    // Strip trailing zeros from the significant digits.
    while digits.len() > 1 && *digits.last().unwrap() == b'0' {
        digits.pop();
    }
    // `{:e}` normalizes to one digit before the point, so the value is
    // `digits[0].digits[1..] * 10^exp`. The decimal point sits after the first
    // digit shifted by `exp`, i.e. dp = exp + 1 (digits-before-point count).
    let dec_point = exp + 1;

    Shortest {
        neg,
        digits,
        dec_point,
    }
}

/// Formats `f` exactly as Go's `encoding/json` encodes a `float64` JSON number.
///
/// Mirrors `floatEncoder.encode` in `encoding/json/encode.go`:
///
/// 1. choose `'e'` (exponent) form when `abs < 1e-6 || abs >= 1e21`, else `'f'`;
/// 2. emit the shortest digits in that form; then
/// 3. for the `'e'` form, rewrite the exponent to strip a leading zero so it has
///    at least **one** digit (`e-05` → `e-5`, `e+09` → `e+9`).
///
/// `f` must be finite. Negative zero renders as `-0` (Go preserves the sign bit
/// here, matching `strconv`).
#[must_use]
pub fn format_json_float(f: f64) -> String {
    let abs = f.abs();
    // Go's threshold is on the magnitude, computed against literal bounds.
    let use_exp = abs != 0.0 && (abs < 1e-6 || abs >= 1e21);
    let s = shortest_decimal(f);
    if use_exp {
        render_exponent(&s, /* min_exp_digits = */ 1)
    } else {
        render_fixed(&s)
    }
}

/// Formats `f` exactly as `strconv.FormatFloat(f, 'g', -1, 64)`.
///
/// The `'g'` format chooses `'e'` when the decimal exponent `exp < -4` or
/// `exp >= 21` (where `exp = dec_point - 1`), else `'f'`. The exponent is
/// rendered with the sign and **at least two** digits (`1e-05`, `1e+21`).
///
/// `f` must be finite.
#[must_use]
pub fn format_float_g(f: f64) -> String {
    let s = shortest_decimal(f);
    if s.digits == [b'0'] {
        // strconv renders ±0 as "0" / "-0".
        return if s.neg { "-0".into() } else { "0".into() };
    }
    // 'g' uses the decimal exponent of the leading digit: value ≈ d * 10^exp.
    let exp = s.dec_point - 1;
    if exp < -4 || exp >= 21 {
        render_exponent(&s, /* min_exp_digits = */ 2)
    } else {
        render_fixed(&s)
    }
}

/// Renders the shortest decimal in fixed (non-exponent) form: `123`, `1.5`,
/// `0.001`, `-0.5`. No trailing zeros, no trailing decimal point.
fn render_fixed(s: &Shortest) -> String {
    let mut out = String::new();
    if s.neg {
        out.push('-');
    }
    let dp = s.dec_point;
    let nd = s.digits.len() as i32;

    if dp <= 0 {
        // 0.00ddd — leading "0." then (-dp) zeros then digits.
        out.push_str("0.");
        for _ in 0..(-dp) {
            out.push('0');
        }
        push_ascii(&mut out, &s.digits);
    } else if dp >= nd {
        // ddd000 — all digits then (dp-nd) trailing zeros, integer-valued.
        push_ascii(&mut out, &s.digits);
        for _ in 0..(dp - nd) {
            out.push('0');
        }
    } else {
        // dd.ddd — split the digits at the decimal point.
        let split = dp as usize;
        push_ascii(&mut out, &s.digits[..split]);
        out.push('.');
        push_ascii(&mut out, &s.digits[split..]);
    }
    out
}

/// Renders the shortest decimal in exponent form: `d` or `d.ddd`, then `e`,
/// sign, and the exponent padded to `min_exp_digits`.
///
/// The mantissa always has exactly one digit before the point (`1`, `1.5`),
/// matching both Go's `encoding/json` and `strconv 'g'`.
fn render_exponent(s: &Shortest, min_exp_digits: usize) -> String {
    let mut out = String::new();
    if s.neg {
        out.push('-');
    }
    out.push(s.digits[0] as char);
    if s.digits.len() > 1 {
        out.push('.');
        push_ascii(&mut out, &s.digits[1..]);
    }
    out.push('e');
    // Exponent of the leading digit.
    let exp = s.dec_point - 1;
    if exp < 0 {
        out.push('-');
    } else {
        out.push('+');
    }
    let mag = exp.unsigned_abs() as u64;
    let mag_str = mag.to_string();
    for _ in mag_str.len()..min_exp_digits {
        out.push('0');
    }
    out.push_str(&mag_str);
    out
}

/// Appends ASCII digit bytes to `out`.
fn push_ascii(out: &mut String, digits: &[u8]) {
    // SAFETY-free: every byte is a validated ASCII digit, so this is plain push.
    for &b in digits {
        out.push(b as char);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_float_threshold_and_exponent_stripping() {
        // >= 1e21 -> exponent, one-digit exponent stripping.
        assert_eq!(format_json_float(1e21), "1e+21");
        assert_eq!(format_json_float(1e20), "100000000000000000000");
        // < 1e-6 -> exponent.
        assert_eq!(format_json_float(1e-7), "1e-7");
        // 1e-5 and 1e-6 are NOT below 1e-6 (1e-6 is the boundary, not <).
        assert_eq!(format_json_float(1e-5), "0.00001");
        assert_eq!(format_json_float(1e-6), "0.000001");
        // integer-valued floats have no decimal point.
        assert_eq!(format_json_float(2.0), "2");
        assert_eq!(format_json_float(100.0), "100");
        // fractions.
        assert_eq!(format_json_float(3.14), "3.14");
        assert_eq!(format_json_float(0.5), "0.5");
        assert_eq!(format_json_float(-0.5), "-0.5");
    }

    #[test]
    fn json_float_negative_zero() {
        assert_eq!(format_json_float(-0.0), "-0");
        assert_eq!(format_json_float(0.0), "0");
    }

    #[test]
    fn json_float_big_integer_valued() {
        // 1.2345678901234568e20 is < 1e21, so fixed form (no decimal point).
        assert_eq!(
            format_json_float(123456789012345680000.0),
            "123456789012345680000"
        );
    }

    #[test]
    fn strconv_g_uses_two_digit_exponent() {
        // 'g' exponent threshold is exp < -4 || exp >= 21.
        assert_eq!(format_float_g(1e21), "1e+21");
        assert_eq!(format_float_g(1e20), "100000000000000000000");
        assert_eq!(format_float_g(1e-5), "1e-05"); // exp = -5 < -4 -> exponent
        assert_eq!(format_float_g(1e-4), "0.0001"); // exp = -4, not < -4 -> fixed
        assert_eq!(format_float_g(1e-7), "1e-07");
        assert_eq!(format_float_g(2.0), "2");
        assert_eq!(format_float_g(3.14), "3.14");
        assert_eq!(format_float_g(-0.0), "-0");
        assert_eq!(format_float_g(0.0), "0");
    }

    #[test]
    fn shortest_decimal_decomposition() {
        let d = shortest_decimal(12.5);
        assert_eq!(d.digits, b"125");
        assert_eq!(d.dec_point, 2); // 12.5 -> "125" with point after 2 digits
        assert!(!d.neg);

        let d = shortest_decimal(0.001);
        assert_eq!(d.digits, b"1");
        assert_eq!(d.dec_point, -2); // 0.001 -> dp = -2

        let d = shortest_decimal(-7.0);
        assert_eq!(d.digits, b"7");
        assert_eq!(d.dec_point, 1);
        assert!(d.neg);
    }

    #[test]
    fn trailing_zeros_stripped() {
        assert_eq!(format_json_float(1.10), "1.1");
        assert_eq!(format_json_float(100.0), "100");
        assert_eq!(format_float_g(1.20), "1.2");
    }
}

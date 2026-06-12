//! The YAML float layout: shortest-precision `'g'` form (frozen contract).
//!
//! This is **not** the JSON float layout (whose exponent threshold is
//! `1e21`/`1e-6`). In the shortest-precision `'g'` form the exponent layout is
//! chosen when `exp < -4 || exp >= 6` where `exp = dp - 1`, otherwise fixed
//! layout. The exponent is rendered with a sign and at least two digits
//! (`1e+20`, `1e-07`).
//!
//! Digits come from Rust's own shortest-round-trip `f64` formatter (`{:e}`),
//! which produces the same digit sequence as the reference formatter (modulo
//! the tie-break corrected below), re-rendered with the `'g'` layout.

/// A parsed shortest-decimal: value = `(-1)^neg * d[0..] * 10^(dp - nd)`.
struct Shortest {
    neg: bool,
    digits: Vec<u8>, // ASCII '0'..='9', no leading/trailing zeros (except "0")
    dp: i32,         // count of digits before the decimal point
}

fn shortest_decimal(f: f64) -> Shortest {
    let neg = f.is_sign_negative();
    let abs = f.abs();
    if abs == 0.0 {
        return Shortest { neg, digits: vec![b'0'], dp: 1 };
    }
    // Rust's `{:e}` => shortest mantissa + decimal exponent, e.g. "1.234e2".
    let s = format!("{abs:e}");
    let (mantissa, exp_str) = s.split_once('e').expect("{:e} has an 'e'");
    let exp: i32 = exp_str.parse().expect("valid exponent");
    let mut digits: Vec<u8> = mantissa.bytes().filter(|&b| b != b'.').collect();
    while digits.len() > 1 && *digits.last().unwrap() == b'0' {
        digits.pop();
    }
    let dp = exp + 1;
    round_half_to_even(&mut digits, dp, abs);
    Shortest { neg, digits, dp }
}

/// Rust's shortest `f64` formatter (`{:e}`) breaks last-digit ties **half-up**,
/// whereas the reference formatter breaks them **half-to-even** (a measured
/// divergence, exercised by the oracle corpus). It only appears for the rare
/// value whose *exact* decimal expansion is `<shortest-digits>` followed by
/// exactly `5000…0` at the next position — a true midpoint. There the contract
/// keeps the even last digit; Rust rounds the odd one up. We detect that
/// exact-midpoint case and, when Rust chose an odd last digit, drop back to
/// the even neighbor.
///
/// `abs` is the positive `f64` these digits came from; `dp` is the count of
/// digits to the left of the decimal point.
fn round_half_to_even(digits: &mut [u8], dp: i32, abs: f64) {
    let last = *digits.last().unwrap();
    if (last - b'0') % 2 == 0 {
        return; // Even already; round-half-to-even would not change it.
    }
    // `abs` is exactly representable in decimal (its denominator is a power of
    // two), so a wide fixed-precision expansion is the *exact* value with no
    // rounding of its own. 40 fractional digits past the leading digit covers
    // every f64 (the longest exact f64 fraction is ~767 digits, but the part we
    // need — one digit beyond the ≤17 significant shortest digits, then a tail
    // that is all zeros for a true midpoint — is captured well within this).
    // We instead compare against the exact value scaled so the shortest result's
    // last digit is the units place.
    let nd = digits.len() as i32;
    // `abs` is dyadic, so a fixed-precision expansion this wide is its *exact*
    // value (the longest f64 fraction is ~1074 places; 1100 covers all).
    let exact = format!("{:.*e}", 1100, abs);
    // `exact` = "d.ddddde±XX"; reconstruct the integer-part-and-fraction at the
    // shortest scale to test the midpoint.
    let (mant, expo) = exact.split_once('e').unwrap();
    let exp: i32 = expo.parse().unwrap();
    let mant_digits: Vec<u8> = mant.bytes().filter(|&b| b != b'.').collect();
    // mant_digits represents value = D.DDD... * 10^exp, i.e. the first digit is
    // at place `exp`. The shortest last digit is at place `dp - nd`. The digit
    // immediately *after* it (the rounding digit) is at place `dp - nd - 1`,
    // whose index in mant_digits is `exp - (dp - nd - 1)`.
    let round_idx = exp - (dp - nd - 1);
    if round_idx < 0 {
        return;
    }
    let round_idx = round_idx as usize;
    if round_idx >= mant_digits.len() {
        return;
    }
    // Midpoint iff the rounding digit is exactly '5' and every following digit
    // is '0' (an exact halfway tie).
    if mant_digits[round_idx] != b'5' {
        return;
    }
    if mant_digits[round_idx + 1..].iter().any(|&b| b != b'0') {
        return; // Not an exact midpoint; Rust's half-up choice is correct.
    }
    // Exact midpoint with an odd last digit -> the contract rounds
    // half-to-even (down).
    *digits.last_mut().unwrap() = last - 1;
}

/// Formats `f` in the contract's shortest-precision `'g'` YAML layout.
///
/// Finite values are the expected input (codefang report floats always are);
/// NaN/±Inf render as the YAML `.nan` / `.inf` / `-.inf` spellings.
#[must_use]
pub fn format_g(f: f64) -> String {
    if f.is_nan() {
        return ".nan".to_string();
    }
    if f.is_infinite() {
        return if f < 0.0 { "-.inf".into() } else { ".inf".into() };
    }
    let s = shortest_decimal(f);
    if s.digits == [b'0'] {
        // The 'g' layout renders both +0 and -0 as "0".
        return "0".to_string();
    }
    // exp = dp - 1; shortest mode fixes the exponent threshold at 6.
    let exp = s.dp - 1;
    if !(-4..6).contains(&exp) {
        render_exp(&s)
    } else {
        render_fixed(&s)
    }
}

/// Fixed layout, e.g. `112539`, `0.7142857142857143`, `0.0001`.
fn render_fixed(s: &Shortest) -> String {
    let mut out = String::new();
    if s.neg {
        out.push('-');
    }
    let nd = s.digits.len() as i32;
    if s.dp <= 0 {
        // 0.00ddd
        out.push_str("0.");
        for _ in 0..(-s.dp) {
            out.push('0');
        }
        for &d in &s.digits {
            out.push(d as char);
        }
    } else if s.dp >= nd {
        // ddd000
        for &d in &s.digits {
            out.push(d as char);
        }
        for _ in 0..(s.dp - nd) {
            out.push('0');
        }
    } else {
        // dd.ddd
        for &d in &s.digits[..s.dp as usize] {
            out.push(d as char);
        }
        out.push('.');
        for &d in &s.digits[s.dp as usize..] {
            out.push(d as char);
        }
    }
    out
}

/// Exponent layout, e.g. `1e+20`, `1e-07`, `1.2345678912345679e+08`.
fn render_exp(s: &Shortest) -> String {
    let mut out = String::new();
    if s.neg {
        out.push('-');
    }
    out.push(s.digits[0] as char);
    if s.digits.len() > 1 {
        out.push('.');
        for &d in &s.digits[1..] {
            out.push(d as char);
        }
    }
    out.push('e');
    let mut e = s.dp - 1;
    if e < 0 {
        out.push('-');
        e = -e;
    } else {
        out.push('+');
    }
    // At least two digits.
    if e < 10 {
        out.push('0');
    }
    out.push_str(&e.to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::format_g;

    #[test]
    fn matches_reference_g_layout() {
        assert_eq!(format_g(1e20), "1e+20");
        assert_eq!(format_g(1e21), "1e+21");
        assert_eq!(format_g(1e-5), "1e-05");
        assert_eq!(format_g(1e-4), "0.0001");
        assert_eq!(format_g(1e-7), "1e-07");
        assert_eq!(format_g(2.0), "2");
        assert_eq!(format_g(3.14), "3.14");
        assert_eq!(format_g(-0.0), "0");
        assert_eq!(format_g(0.0), "0");
        assert_eq!(format_g(1.20), "1.2");
        assert_eq!(format_g(112539.0), "112539");
        assert_eq!(format_g(0.7142857142857143), "0.7142857142857143");
        assert_eq!(format_g(123456789.123456789), "1.2345678912345679e+08");
        assert_eq!(format_g(2.5e-10), "2.5e-10");
        assert_eq!(format_g(1234567.0), "1.234567e+06");
        assert_eq!(format_g(0.000123), "0.000123");
        assert_eq!(format_g(100.0), "100");
        assert_eq!(format_g(0.1), "0.1");
        assert_eq!(format_g(2.5e-10), "2.5e-10");
        assert_eq!(format_g(1.5), "1.5");
        assert_eq!(format_g(0.23943661971830985), "0.23943661971830985");
    }
}

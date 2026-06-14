//! Plain-scalar resolution — decides whether an unquoted string would parse
//! back as a non-`!!str` tag (and thus must be quoted on output).
//!
//! [`string_can_use_plain`] is the single entry point: a string may stay plain
//! only when it resolves to `!!str` and is neither a legacy base-60 float nor
//! a legacy boolean spelling. The classification rules are part of the frozen
//! emitter contract.

/// Returns true if the plain (unquoted) form of `s` still resolves to `!!str`
/// and is not a YAML-1.1 base-60 float or old-style bool, i.e. it is safe to
/// emit without quotes (the emitter may still pick single/double quotes for
/// structural reasons).
#[must_use]
pub fn string_can_use_plain(s: &str) -> bool {
    resolves_to_str(s) && !is_base60_float(s) && !is_old_bool(s)
}

/// Resolution hint for the first byte: sign, digit, maybe-special, or dot.
fn hint(c: u8) -> u8 {
    match c {
        b'+' | b'-' => b'S',
        b'0'..=b'9' => b'D',
        b'y' | b'Y' | b'n' | b'N' | b't' | b'T' | b'f' | b'F' | b'o' | b'O' | b'~' => b'M',
        b'.' => b'.',
        _ => 0,
    }
}

/// Lookup of the small set of fully-spelled special scalars. Returns
/// Some(true) when the string maps to a NON-str tag (bool/null/float).
fn resolve_map_is_non_str(s: &str) -> Option<bool> {
    match s {
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" | "" | "~" | "null" | "Null"
        | "NULL" | ".nan" | ".NaN" | ".NAN" | ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf"
        | "+.INF" | "-.inf" | "-.Inf" | "-.INF" => Some(true),
        // `<<` resolves to the merge tag, but the reference emitter writes it
        // *plain* in value position (`k: <<`); only the rarely-reachable key
        // path adds `!!merge`. codefang reports never contain `<<`, so it is
        // treated as a plain string, matching the value-context output.
        _ => None,
    }
}

/// Whether `s` resolves to the string tag (i.e. the value stays a string).
fn resolves_to_str(s: &str) -> bool {
    let h = if s.is_empty() { b'N' } else { hint(s.as_bytes()[0]) };
    if h == 0 {
        return true;
    }
    // resolveMap lookup first.
    if let Some(non_str) = resolve_map_is_non_str(s) {
        // Anything found in resolveMap is non-str (bool/null/float/merge).
        return !non_str;
    }
    match h {
        b'M' => {
            // Already checked the map above; nothing else matches -> str.
            true
        }
        b'.' => {
            // Maybe a normal float.
            !parse_float_ok(s)
        }
        b'D' | b'S' => {
            // Timestamp?
            if parse_timestamp(s) {
                return false;
            }
            let plain = s.replace('_', "");
            if parse_int_go(&plain) {
                return false;
            }
            if parse_uint_go(&plain) {
                return false;
            }
            if yaml_style_float(&plain) && parse_float_ok(&plain) {
                return false;
            }
            if let Some(rest) = plain.strip_prefix("0b") {
                if parse_radix_signed(rest, 2) || parse_radix_unsigned(rest, 2) {
                    return false;
                }
            } else if let Some(rest) = plain.strip_prefix("-0b") {
                if parse_radix_signed_neg(rest, 2) {
                    return false;
                }
            }
            if let Some(rest) = plain.strip_prefix("0o") {
                if parse_radix_signed(rest, 8) || parse_radix_unsigned(rest, 8) {
                    return false;
                }
            } else if let Some(rest) = plain.strip_prefix("-0o") {
                if parse_radix_signed_neg(rest, 8) {
                    return false;
                }
            }
            true
        }
        _ => true,
    }
}

/// Whether `s` parses as an `i64` with base auto-detection (0x/0o/0b prefixes
/// and leading-zero octal).
fn parse_int_go(s: &str) -> bool {
    parse_int_base0(s).is_some()
}

fn parse_uint_go(s: &str) -> bool {
    let t = s.strip_prefix('+').unwrap_or(s);
    if t.starts_with('-') {
        return false;
    }
    parse_uint_base0(t).is_some()
}

fn parse_int_base0(s: &str) -> Option<i64> {
    let (neg, body) = if let Some(b) = s.strip_prefix('-') {
        (true, b)
    } else if let Some(b) = s.strip_prefix('+') {
        (false, b)
    } else {
        (false, s)
    };
    let v = parse_uint_base0(body)? as i128;
    let v = if neg { -v } else { v };
    if v >= i64::MIN as i128 && v <= i64::MAX as i128 {
        Some(v as i64)
    } else {
        None
    }
}

/// Parse an unsigned integer with base-0 prefix detection (`0x`/`0o`/`0b`,
/// and a bare leading `0` meaning octal).
fn parse_uint_base0(s: &str) -> Option<u128> {
    if s.is_empty() {
        return None;
    }
    let (radix, digits): (u32, &str) = if let Some(r) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, r)
    } else if let Some(r) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        (8, r)
    } else if let Some(r) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        (2, r)
    } else if s.len() > 1 && s.starts_with('0') {
        (8, &s[1..])
    } else {
        (10, s)
    };
    if digits.is_empty() {
        return None;
    }
    // The reference parser allows underscores only adjacent to digits; report
    // strings never hit those subtleties, so a plain strict parse suffices.
    let mut acc: u128 = 0;
    for c in digits.chars() {
        let d = c.to_digit(radix)?;
        acc = acc.checked_mul(radix as u128)?.checked_add(d as u128)?;
    }
    Some(acc)
}

fn parse_radix_signed(s: &str, radix: u32) -> bool {
    i64::from_str_radix(s, radix).is_ok()
}
fn parse_radix_unsigned(s: &str, radix: u32) -> bool {
    u64::from_str_radix(s, radix).is_ok()
}
fn parse_radix_signed_neg(s: &str, radix: u32) -> bool {
    i64::from_str_radix(&format!("-{s}"), radix).is_ok()
}

fn parse_float_ok(s: &str) -> bool {
    // The reference float parser accepts inf/nan spellings too, but those are
    // caught by the special-scalar map above. A plain Rust parse matches for
    // the numeric forms that matter here.
    s.parse::<f64>().is_ok()
}

/// YAML-style float pattern: `^[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?$`.
fn yaml_style_float(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    // mantissa: either .D+ or D+(.D*)?
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    } else {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    // optional exponent
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == b.len()
}

/// Legacy (YAML 1.1) base-60 float pattern:
/// `^[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+(?:\.[0-9_]*)?$`.
fn is_base60_float(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let c = s.as_bytes()[0];
    if !(c == b'+' || c == b'-' || c.is_ascii_digit()) || !s.contains(':') {
        return false;
    }
    let b = s.as_bytes();
    let mut i = 0;
    if b[i] == b'+' || b[i] == b'-' {
        i += 1;
    }
    if i >= b.len() || !b[i].is_ascii_digit() {
        return false;
    }
    i += 1;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
        i += 1;
    }
    // (?::[0-5]?[0-9])+
    let mut groups = 0;
    while i < b.len() && b[i] == b':' {
        i += 1;
        let start = i;
        // optional [0-5]
        if i < b.len() && (b'0'..=b'5').contains(&b[i]) {
            // could be the tens digit; consume one more if a digit follows
            if i + 1 < b.len() && b[i + 1].is_ascii_digit() {
                i += 1;
            }
        }
        if i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        } else {
            return false;
        }
        if i == start {
            return false;
        }
        groups += 1;
    }
    if groups == 0 {
        return false;
    }
    // optional (?:\.[0-9_]*)
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
            i += 1;
        }
    }
    i == b.len()
}

/// Legacy (YAML 1.1) boolean spellings (`yes`/`no`/`on`/`off` families).
fn is_old_bool(s: &str) -> bool {
    matches!(
        s,
        "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" | "n" | "N" | "no" | "No" | "NO"
            | "off" | "Off" | "OFF"
    )
}

/// Timestamp detection: quick year check + the four allowed layouts.
fn parse_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i != 4 || i == b.len() || b[i] != b'-' {
        return false;
    }
    // Layouts (1-2 digit month/day/hour/min/sec fields):
    //   YYYY-M-DTH:M:S(.frac)?Z|±hh:mm   (RFC3339-style, 'T')
    //   YYYY-M-DtH:M:S(.frac)?Z|±hh:mm   (lower 't')
    //   YYYY-M-D H:M:S(.frac)?           (space, no zone)
    //   YYYY-M-D                         (date only)
    parse_date_only(s)
        || parse_datetime(s, b'T', true)
        || parse_datetime(s, b't', true)
        || parse_datetime(s, b' ', false)
}

fn parse_date_only(s: &str) -> bool {
    parse_ymd(s).is_some_and(|rest| rest.is_empty())
}

/// Parses `YYYY-M-D` (1-2 digit month/day), returning the remainder.
fn parse_ymd(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i != 4 || i >= b.len() || b[i] != b'-' {
        return None;
    }
    i += 1;
    let m = take_digits(b, i, 1, 2)?;
    i = m;
    if i >= b.len() || b[i] != b'-' {
        return None;
    }
    i += 1;
    let d = take_digits(b, i, 1, 2)?;
    i = d;
    Some(&s[i..])
}

fn parse_datetime(s: &str, sep: u8, with_zone: bool) -> bool {
    let Some(rest) = parse_ymd(s) else {
        return false;
    };
    let rb = rest.as_bytes();
    if rb.is_empty() || rb[0] != sep {
        return false;
    }
    let mut i = 1;
    // H:M:S(.frac)?
    let Some(h) = take_digits(rb, i, 1, 2) else {
        return false;
    };
    i = h;
    if i >= rb.len() || rb[i] != b':' {
        return false;
    }
    i += 1;
    let Some(mi) = take_digits(rb, i, 1, 2) else {
        return false;
    };
    i = mi;
    if i >= rb.len() || rb[i] != b':' {
        return false;
    }
    i += 1;
    let Some(se) = take_digits(rb, i, 1, 2) else {
        return false;
    };
    i = se;
    if i < rb.len() && rb[i] == b'.' {
        i += 1;
        let start = i;
        while i < rb.len() && rb[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    if with_zone {
        // Z or ±hh:mm
        if i >= rb.len() {
            return false;
        }
        if rb[i] == b'Z' {
            i += 1;
        } else if rb[i] == b'+' || rb[i] == b'-' {
            i += 1;
            let Some(z1) = take_digits(rb, i, 2, 2) else {
                return false;
            };
            i = z1;
            if i >= rb.len() || rb[i] != b':' {
                return false;
            }
            i += 1;
            let Some(z2) = take_digits(rb, i, 2, 2) else {
                return false;
            };
            i = z2;
        } else {
            return false;
        }
    }
    i == rb.len()
}

/// Consumes between `min` and `max` ASCII digits starting at `i`, returning the
/// new index, or None if fewer than `min`.
fn take_digits(b: &[u8], i: usize, min: usize, max: usize) -> Option<usize> {
    let mut j = i;
    while j < b.len() && j - i < max && b[j].is_ascii_digit() {
        j += 1;
    }
    if j - i >= min {
        Some(j)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::string_can_use_plain;

    fn plain(s: &str) -> bool {
        string_can_use_plain(s)
    }

    #[test]
    fn numbers_bools_null_quoted() {
        for s in ["true", "false", "123", "0", "+5", "-3", "1.5", "null", "~", "", "yes", "no", "on", "off", "1e3", ".5", "0x1f", "0o17", "0b101"] {
            assert!(!plain(s), "{s:?} should NOT be plain");
        }
    }

    #[test]
    fn timestamps_quoted() {
        for s in ["2026-01-26T21:53:53Z", "2026-1-2", "2026-01-26 21:53:53", "2026-01-26t21:53:53Z"] {
            assert!(!plain(s), "{s:?} should NOT be plain (timestamp)");
        }
    }

    #[test]
    fn strings_plain() {
        for s in ["hello", "<unknown>", "CRITICAL", "a:b", "k8s.io/x", "123abc", ".", "go", "external", "say \"hi\"", "it's", "@foo", "!x"] {
            assert!(plain(s), "{s:?} should be plain (resolve to str)");
        }
    }
}

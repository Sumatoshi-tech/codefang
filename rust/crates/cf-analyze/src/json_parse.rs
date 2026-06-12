//! Minimal JSON parser producing [`cf_gojson::GoValue`].
//!
//! `cf-gojson` is an **encoder** (its job is byte-identical output). The
//! conversion hub also needs to *parse* canonical JSON input
//! (`ParseUnifiedModelJSON`, `DecodeCombinedBinaryReports`). Parsing is not
//! byte-identity relevant — only re-encoding is — so a small standards-compliant
//! parser into [`GoValue`] suffices. Integers that fit in `i64`/`u64` are kept
//! as [`GoValue::Int`]/[`GoValue::Uint`]; anything with a `.`/`e` becomes
//! [`GoValue::Float`], matching how the reference decoder (`json.Unmarshal` into `any`) would later
//! re-marshal (the reference decoder uses doubles for all numbers, but report values
//! round-trip through this typed model; integer preservation keeps re-encoded
//! counts free of spurious decimals).
//!
//! Object values are stored in **map-origin** [`GoMap`]s so that any
//! re-encoding byte-sorts keys per the report-format contract.

use cf_gojson::{GoMap, GoValue};

/// A JSON parse error with a byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable message.
    pub message: String,
    /// Byte offset where the error was detected.
    pub offset: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "json parse error at {}: {}", self.offset, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parses `input` as a single JSON value into a [`GoValue`].
pub fn parse(input: &[u8]) -> Result<GoValue, ParseError> {
    let mut p = Parser { b: input, i: 0 };
    p.skip_ws();
    let v = p.parse_value()?;
    p.skip_ws();
    if p.i != p.b.len() {
        return Err(p.err("trailing data after JSON value"));
    }
    Ok(v)
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, msg: &str) -> ParseError {
        ParseError { message: msg.to_string(), offset: self.i }
    }

    fn skip_ws(&mut self) {
        while self.i < self.b.len() {
            match self.b[self.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    fn parse_value(&mut self) -> Result<GoValue, ParseError> {
        self.skip_ws();
        if self.i >= self.b.len() {
            return Err(self.err("unexpected end of input"));
        }
        match self.b[self.i] {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => Ok(GoValue::Str(self.parse_string()?)),
            b't' | b'f' => self.parse_bool(),
            b'n' => self.parse_null(),
            b'-' | b'0'..=b'9' => self.parse_number(),
            _ => Err(self.err("unexpected character")),
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), ParseError> {
        self.skip_ws();
        if self.i < self.b.len() && self.b[self.i] == c {
            self.i += 1;
            Ok(())
        } else {
            Err(self.err("unexpected character"))
        }
    }

    fn parse_object(&mut self) -> Result<GoValue, ParseError> {
        self.expect(b'{')?;
        let mut obj = GoMap::new_map();
        self.skip_ws();
        if self.i < self.b.len() && self.b[self.i] == b'}' {
            self.i += 1;
            return Ok(GoValue::Object(obj));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.expect(b':')?;
            let val = self.parse_value()?;
            obj.push(key, val);
            self.skip_ws();
            if self.i >= self.b.len() {
                return Err(self.err("unterminated object"));
            }
            match self.b[self.i] {
                b',' => {
                    self.i += 1;
                    continue;
                }
                b'}' => {
                    self.i += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or '}'")),
            }
        }
        Ok(GoValue::Object(obj))
    }

    fn parse_array(&mut self) -> Result<GoValue, ParseError> {
        self.expect(b'[')?;
        let mut arr = Vec::new();
        self.skip_ws();
        if self.i < self.b.len() && self.b[self.i] == b']' {
            self.i += 1;
            return Ok(GoValue::Array(arr));
        }
        loop {
            let val = self.parse_value()?;
            arr.push(val);
            self.skip_ws();
            if self.i >= self.b.len() {
                return Err(self.err("unterminated array"));
            }
            match self.b[self.i] {
                b',' => {
                    self.i += 1;
                    continue;
                }
                b']' => {
                    self.i += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or ']'")),
            }
        }
        Ok(GoValue::Array(arr))
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        if self.i >= self.b.len() || self.b[self.i] != b'"' {
            return Err(self.err("expected string"));
        }
        self.i += 1;
        let mut s = String::new();
        loop {
            if self.i >= self.b.len() {
                return Err(self.err("unterminated string"));
            }
            let c = self.b[self.i];
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    if self.i >= self.b.len() {
                        return Err(self.err("unterminated escape"));
                    }
                    let e = self.b[self.i];
                    self.i += 1;
                    match e {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'/' => s.push('/'),
                        b'b' => s.push('\u{0008}'),
                        b'f' => s.push('\u{000C}'),
                        b'n' => s.push('\n'),
                        b'r' => s.push('\r'),
                        b't' => s.push('\t'),
                        b'u' => {
                            let cp = self.parse_hex4()?;
                            if (0xD800..=0xDBFF).contains(&cp) {
                                // High surrogate; expect a following \uXXXX low.
                                if self.i + 1 < self.b.len()
                                    && self.b[self.i] == b'\\'
                                    && self.b[self.i + 1] == b'u'
                                {
                                    self.i += 2;
                                    let low = self.parse_hex4()?;
                                    let combined = 0x10000
                                        + (((cp - 0xD800) as u32) << 10)
                                        + (low - 0xDC00) as u32;
                                    if let Some(ch) = char::from_u32(combined) {
                                        s.push(ch);
                                    } else {
                                        s.push('\u{FFFD}');
                                    }
                                } else {
                                    s.push('\u{FFFD}');
                                }
                            } else if let Some(ch) = char::from_u32(cp as u32) {
                                s.push(ch);
                            } else {
                                s.push('\u{FFFD}');
                            }
                        }
                        _ => return Err(self.err("invalid escape")),
                    }
                }
                _ => {
                    // Collect a UTF-8 sequence starting at c.
                    let start = self.i - 1;
                    let len = utf8_len(c);
                    if len == 0 {
                        return Err(self.err("invalid UTF-8"));
                    }
                    let end = start + len;
                    if end > self.b.len() {
                        return Err(self.err("truncated UTF-8"));
                    }
                    match std::str::from_utf8(&self.b[start..end]) {
                        Ok(seg) => s.push_str(seg),
                        Err(_) => return Err(self.err("invalid UTF-8")),
                    }
                    self.i = end;
                }
            }
        }
        Ok(s)
    }

    fn parse_hex4(&mut self) -> Result<u16, ParseError> {
        if self.i + 4 > self.b.len() {
            return Err(self.err("truncated \\u escape"));
        }
        let mut v: u16 = 0;
        for _ in 0..4 {
            let d = self.b[self.i];
            self.i += 1;
            let digit = match d {
                b'0'..=b'9' => d - b'0',
                b'a'..=b'f' => d - b'a' + 10,
                b'A'..=b'F' => d - b'A' + 10,
                _ => return Err(self.err("invalid hex digit")),
            };
            v = (v << 4) | digit as u16;
        }
        Ok(v)
    }

    fn parse_bool(&mut self) -> Result<GoValue, ParseError> {
        if self.b[self.i..].starts_with(b"true") {
            self.i += 4;
            Ok(GoValue::Bool(true))
        } else if self.b[self.i..].starts_with(b"false") {
            self.i += 5;
            Ok(GoValue::Bool(false))
        } else {
            Err(self.err("invalid literal"))
        }
    }

    fn parse_null(&mut self) -> Result<GoValue, ParseError> {
        if self.b[self.i..].starts_with(b"null") {
            self.i += 4;
            Ok(GoValue::Null)
        } else {
            Err(self.err("invalid literal"))
        }
    }

    fn parse_number(&mut self) -> Result<GoValue, ParseError> {
        let start = self.i;
        let mut is_float = false;
        if self.i < self.b.len() && self.b[self.i] == b'-' {
            self.i += 1;
        }
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'0'..=b'9' => self.i += 1,
                b'.' | b'e' | b'E' | b'+' | b'-' => {
                    is_float = true;
                    self.i += 1;
                }
                _ => break,
            }
        }
        let text = std::str::from_utf8(&self.b[start..self.i])
            .map_err(|_| self.err("invalid number"))?;
        if !is_float {
            if let Ok(i) = text.parse::<i64>() {
                return Ok(GoValue::Int(i));
            }
            if let Ok(u) = text.parse::<u64>() {
                return Ok(GoValue::Uint(u));
            }
        }
        text.parse::<f64>()
            .map(GoValue::Float)
            .map_err(|_| self.err("invalid number"))
    }
}

const fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else if first >> 3 == 0b11110 {
        4
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_object_sorted() {
        let v = parse(br#"{"b":2,"a":1}"#).unwrap();
        let out = cf_gojson::marshal(&v);
        assert_eq!(out, br#"{"a":1,"b":2}"#);
    }

    #[test]
    fn parse_scalars() {
        assert_eq!(parse(b"true").unwrap(), GoValue::Bool(true));
        assert_eq!(parse(b"null").unwrap(), GoValue::Null);
        assert_eq!(parse(b"42").unwrap(), GoValue::Int(42));
        assert_eq!(parse(b"-7").unwrap(), GoValue::Int(-7));
        assert_eq!(parse(br#""hi""#).unwrap(), GoValue::Str("hi".into()));
        match parse(b"2.75").unwrap() {
            GoValue::Float(f) => assert!((f - 2.75).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn parse_nested() {
        let v = parse(br#"{"a":[1,2,{"x":"y"}]}"#).unwrap();
        let out = cf_gojson::marshal(&v);
        assert_eq!(out, br#"{"a":[1,2,{"x":"y"}]}"#);
    }

    #[test]
    fn parse_escapes() {
        let v = parse(br#""a\nbA""#).unwrap();
        assert_eq!(v, GoValue::Str("a\nbA".into()));
    }

    #[test]
    fn trailing_data_errors() {
        assert!(parse(b"1 2").is_err());
    }
}

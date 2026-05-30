//! Go-compatible JSON codec.
//!
//! [`JsonCodec`] reproduces the byte output of Go's `encoding/json` encoder with
//! `SetEscapeHTML(true)`. The escaping and key-ordering rules are implemented by
//! [`GoCompatFormatter`], which higher-level report serializers can reuse to keep
//! their output byte-for-byte identical to the Go pipeline.
//!
//! # Parity notes vs. Go `encoding/json`
//!
//! * **HTML escaping.** `<`, `>` and `&` are escaped as `<`, `>` and
//!   `&`. Go does this whenever `SetEscapeHTML(true)` is in effect (the
//!   default for `json.Marshal` and what the Go `persist.JSONCodec` opts into).
//! * **Line/paragraph separators.** U+2028 and U+2029 are escaped as ` `
//!   and ` `. Go always escapes these regardless of the HTML-escape flag,
//!   because they are valid JSON but invalid in JavaScript string literals.
//! * **Map key ordering.** Go sorts object keys produced from maps. `serde_json`
//!   (built without the `preserve_order` feature) stores `Value::Object` in a
//!   `BTreeMap`, so dynamic objects serialize with sorted keys too. Struct
//!   fields keep declaration order in both languages.
//! * **Trailing newline.** Go's `json.Encoder.Encode` appends a `\n` after every
//!   value. [`JsonCodec::encode`] does the same.
//! * **Indentation.** When [`JsonCodec`] has a non-empty indent string, output
//!   is pretty-printed using that string, matching `enc.SetIndent("", indent)`.

use std::io;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter, PrettyFormatter};

use crate::error::{Error, Result};
use crate::Codec;

/// A JSON codec whose output matches Go's `encoding/json` with HTML escaping on.
///
/// Construct with [`JsonCodec::compact`] for dense output (no indentation) or
/// [`JsonCodec::indented`] / [`JsonCodec::with_indent`] for pretty-printed
/// output. Either way, encoded output always ends with a trailing newline.
///
/// This corresponds to the Go `persist.JSONCodec` struct, where an empty
/// `Indent` field selects compact output and a non-empty one selects
/// pretty-printing.
#[derive(Debug, Clone, Default)]
pub struct JsonCodec {
    /// When non-empty, output is pretty-printed using this string as one level
    /// of indentation. An empty string selects compact output.
    indent: Vec<u8>,
}

impl JsonCodec {
    /// Creates a codec that produces compact JSON (no indentation).
    ///
    /// Equivalent to the Go `persist.JSONCodec{}` zero value.
    pub fn compact() -> Self {
        Self { indent: Vec::new() }
    }

    /// Creates a codec that pretty-prints using two spaces per level.
    ///
    /// Equivalent to the Go `persist.JSONCodec{Indent: "  "}` used by the shared
    /// `common.JSONIndented` codec.
    pub fn indented() -> Self {
        Self::with_indent("  ")
    }

    /// Creates a codec that pretty-prints using the given indentation string.
    ///
    /// An empty `indent` selects compact output, matching the Go behavior where
    /// `Indent == ""` disables pretty-printing.
    pub fn with_indent(indent: impl AsRef<str>) -> Self {
        Self {
            indent: indent.as_ref().as_bytes().to_vec(),
        }
    }

    /// Reports whether this codec pretty-prints (i.e. has a non-empty indent).
    pub fn is_indented(&self) -> bool {
        !self.indent.is_empty()
    }

    /// Encodes `value` as JSON bytes (the inherent form of [`Codec::encode`]).
    pub fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(128);
        if self.indent.is_empty() {
            let formatter = GoCompatFormatter::new(CompactFormatter);
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
            value.serialize(&mut ser).map_err(Error::JsonEncode)?;
        } else {
            let pretty = PrettyFormatter::with_indent(&self.indent);
            let formatter = GoCompatFormatter::new(pretty);
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
            value.serialize(&mut ser).map_err(Error::JsonEncode)?;
        }
        // Go's json.Encoder.Encode always appends a trailing newline.
        buf.push(b'\n');
        Ok(buf)
    }

    /// Decodes JSON `data` into a value of type `T` (the inherent form of
    /// [`Codec::decode`]).
    pub fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> Result<T> {
        serde_json::from_slice(data).map_err(Error::JsonDecode)
    }
}

impl Codec for JsonCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        JsonCodec::encode(self, value)
    }

    fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> Result<T> {
        JsonCodec::decode(self, data)
    }
}

/// A `serde_json` [`Formatter`] that applies Go's `encoding/json` escaping rules.
///
/// It wraps any inner formatter (e.g. [`CompactFormatter`] or
/// [`PrettyFormatter`]) and only customizes how string *contents* are written,
/// so structural layout (indentation, separators) is delegated to the inner
/// formatter. Within string contents it additionally escapes `<`, `>`, `&`,
/// U+2028 and U+2029, exactly as Go does.
#[derive(Debug, Clone)]
pub struct GoCompatFormatter<F> {
    inner: F,
}

impl<F> GoCompatFormatter<F> {
    /// Wraps `inner` with Go-compatible string escaping.
    pub fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F: Formatter> Formatter for GoCompatFormatter<F> {
    fn write_string_fragment<W>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        // serde_json hands us runs of characters it considers "safe" (no control
        // chars, quotes or backslashes). Go additionally escapes the HTML
        // metacharacters and the U+2028 / U+2029 separators, so we re-scan the
        // fragment and split on those.
        let bytes = fragment.as_bytes();
        let mut start = 0;
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            match b {
                b'<' | b'>' | b'&' => {
                    if start < i {
                        // Safe: this is a sub-slice of valid UTF-8 on char
                        // boundaries (all bytes so far were ASCII < 0x80).
                        self.inner
                            .write_string_fragment(writer, &fragment[start..i])?;
                    }
                    write_u4_escape(writer, b as u16)?;
                    i += 1;
                    start = i;
                }
                0xE2 if i + 2 < bytes.len()
                    && bytes[i + 1] == 0x80
                    && (bytes[i + 2] == 0xA8 || bytes[i + 2] == 0xA9) =>
                {
                    // U+2028 (0xE2 0x80 0xA8) and U+2029 (0xE2 0x80 0xA9).
                    if start < i {
                        self.inner
                            .write_string_fragment(writer, &fragment[start..i])?;
                    }
                    let cp = if bytes[i + 2] == 0xA8 { 0x2028 } else { 0x2029 };
                    write_u4_escape(writer, cp)?;
                    i += 3;
                    start = i;
                }
                _ => {
                    i += 1;
                }
            }
        }
        if start < bytes.len() {
            self.inner
                .write_string_fragment(writer, &fragment[start..])?;
        }
        Ok(())
    }

    // --- everything else is delegated verbatim to the inner formatter ---

    #[inline]
    fn write_null<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_null(writer)
    }

    #[inline]
    fn write_bool<W>(&mut self, writer: &mut W, value: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_bool(writer, value)
    }

    #[inline]
    fn write_i8<W>(&mut self, writer: &mut W, value: i8) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i8(writer, value)
    }

    #[inline]
    fn write_i16<W>(&mut self, writer: &mut W, value: i16) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i16(writer, value)
    }

    #[inline]
    fn write_i32<W>(&mut self, writer: &mut W, value: i32) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i32(writer, value)
    }

    #[inline]
    fn write_i64<W>(&mut self, writer: &mut W, value: i64) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i64(writer, value)
    }

    #[inline]
    fn write_i128<W>(&mut self, writer: &mut W, value: i128) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_i128(writer, value)
    }

    #[inline]
    fn write_u8<W>(&mut self, writer: &mut W, value: u8) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u8(writer, value)
    }

    #[inline]
    fn write_u16<W>(&mut self, writer: &mut W, value: u16) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u16(writer, value)
    }

    #[inline]
    fn write_u32<W>(&mut self, writer: &mut W, value: u32) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u32(writer, value)
    }

    #[inline]
    fn write_u64<W>(&mut self, writer: &mut W, value: u64) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u64(writer, value)
    }

    #[inline]
    fn write_u128<W>(&mut self, writer: &mut W, value: u128) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_u128(writer, value)
    }

    #[inline]
    fn write_f32<W>(&mut self, writer: &mut W, value: f32) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_f32(writer, value)
    }

    #[inline]
    fn write_f64<W>(&mut self, writer: &mut W, value: f64) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_f64(writer, value)
    }

    #[inline]
    fn write_number_str<W>(&mut self, writer: &mut W, value: &str) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_number_str(writer, value)
    }

    #[inline]
    fn begin_string<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_string(writer)
    }

    #[inline]
    fn end_string<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_string(writer)
    }

    #[inline]
    fn write_char_escape<W>(
        &mut self,
        writer: &mut W,
        char_escape: serde_json::ser::CharEscape,
    ) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_char_escape(writer, char_escape)
    }

    #[inline]
    fn begin_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_array(writer)
    }

    #[inline]
    fn end_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_array(writer)
    }

    #[inline]
    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_array_value(writer, first)
    }

    #[inline]
    fn end_array_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_array_value(writer)
    }

    #[inline]
    fn begin_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_object(writer)
    }

    #[inline]
    fn end_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_object(writer)
    }

    #[inline]
    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_object_key(writer, first)
    }

    #[inline]
    fn end_object_key<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_object_key(writer)
    }

    #[inline]
    fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.begin_object_value(writer)
    }

    #[inline]
    fn end_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.end_object_value(writer)
    }

    #[inline]
    fn write_raw_fragment<W>(&mut self, writer: &mut W, fragment: &str) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.inner.write_raw_fragment(writer, fragment)
    }
}

/// Writes a `\uXXXX` escape for the given code unit, lowercase-hex, as Go does.
fn write_u4_escape<W>(writer: &mut W, code: u16) -> io::Result<()>
where
    W: ?Sized + io::Write,
{
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let buf = [
        b'\\',
        b'u',
        HEX[((code >> 12) & 0xF) as usize],
        HEX[((code >> 8) & 0xF) as usize],
        HEX[((code >> 4) & 0xF) as usize],
        HEX[(code & 0xF) as usize],
    ];
    writer.write_all(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Sample {
        name: String,
        value: i64,
    }

    // Ported from Go TestJSONCodecEncodeDecode.
    #[test]
    fn json_codec_encode_decode_round_trips() {
        let codec = JsonCodec::compact();
        let original = Sample {
            name: "test".into(),
            value: 42,
        };

        let data = codec.encode(&original).expect("encode");
        let decoded: Sample = codec.decode(&data).expect("decode");

        assert_eq!(decoded, original);
    }

    // Ported from Go TestJSONCodecEncodeWithIndent.
    #[test]
    fn json_codec_encode_with_indent_contains_newlines() {
        let codec = JsonCodec::indented();
        let mut map = std::collections::BTreeMap::new();
        map.insert("a", 1);
        let data = codec.encode(&map).expect("encode");
        assert!(
            data.windows(1).any(|w| w == b"\n"),
            "expected indented output to contain newlines"
        );
    }

    #[test]
    fn compact_output_matches_go_bytes() {
        let codec = JsonCodec::compact();
        let original = Sample {
            name: "test".into(),
            value: 42,
        };
        let data = codec.encode(&original).expect("encode");
        // Go: json.Encoder with compact output + trailing newline.
        assert_eq!(data, b"{\"name\":\"test\",\"value\":42}\n");
    }

    #[test]
    fn html_metacharacters_are_escaped_like_go() {
        let codec = JsonCodec::compact();
        let mut map = std::collections::BTreeMap::new();
        map.insert("k", "<a>&\"b\"");
        let data = codec.encode(&map).expect("encode");
        // Verified against Go: {"k":"<a>&\"b\""}\n
        assert_eq!(data, b"{\"k\":\"\\u003ca\\u003e\\u0026\\\"b\\\"\"}\n");
    }

    #[test]
    fn line_and_paragraph_separators_are_escaped() {
        let codec = JsonCodec::compact();
        let mut map = std::collections::BTreeMap::new();
        // U+2028 and U+2029.
        map.insert("k", "x\u{2028}y\u{2029}z");
        let data = codec.encode(&map).expect("encode");
        assert_eq!(
            String::from_utf8(data).unwrap(),
            "{\"k\":\"x\\u2028y\\u2029z\"}\n"
        );
    }

    #[test]
    fn indented_output_matches_go_bytes() {
        let codec = JsonCodec::indented();
        let mut map = std::collections::BTreeMap::new();
        map.insert("a", 1);
        let data = codec.encode(&map).expect("encode");
        // Verified against Go: enc.SetIndent("", "  ") => "{\n  \"a\": 1\n}\n"
        assert_eq!(String::from_utf8(data).unwrap(), "{\n  \"a\": 1\n}\n");
    }

    #[test]
    fn map_keys_are_sorted_like_go() {
        let codec = JsonCodec::compact();
        let mut map = std::collections::BTreeMap::new();
        map.insert("b", 2);
        map.insert("a", 1);
        map.insert("c", 3);
        let data = codec.encode(&map).expect("encode");
        assert_eq!(data, b"{\"a\":1,\"b\":2,\"c\":3}\n");
    }

    #[test]
    fn decode_rejects_invalid_json() {
        let codec = JsonCodec::compact();
        let err = codec.decode::<Sample>(b"{not json}").unwrap_err();
        assert!(matches!(err, Error::JsonDecode(_)));
        assert!(err.to_string().starts_with("persist: json decode:"));
    }

    #[test]
    fn empty_indent_string_selects_compact() {
        let codec = JsonCodec::with_indent("");
        assert!(!codec.is_indented());
        let mut map = std::collections::BTreeMap::new();
        map.insert("a", 1);
        let data = codec.encode(&map).expect("encode");
        assert_eq!(data, b"{\"a\":1}\n");
    }
}

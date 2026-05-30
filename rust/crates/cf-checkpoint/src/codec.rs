//! Serialization codecs and atomic state persistence.
//!
//! Ported from Go's `pkg/persist` (re-exported by `internal/checkpoint/codec.go`
//! and `persister.go`). The Go package defines a small `Codec` interface plus
//! `SaveState` / `LoadState` free functions and a generic `Persister[T]`.
//!
//! # Relationship to `cf-persist`
//!
//! The design (§1) places these codecs in a shared `cf-persist` crate that
//! routes JSON output through `cf-gojson`. That crate exists but is not yet a
//! buildable workspace member, so per the port rules this module defines the
//! minimal codec surface checkpointing needs. The [`Codec`] trait below is
//! exactly the interface that crate must satisfy; once it compiles this module
//! collapses to a thin re-export of `cf_persist::{Codec, JsonCodec, GobCodec,
//! save_state, load_state, Persister}` with no behavior change.
//!
//! # Byte-identity of the JSON metadata
//!
//! Go writes checkpoint metadata with `json.NewEncoder(w)` + `SetIndent("","  ")`
//! (`persist.JSONCodec`). That encoder:
//!
//! * HTML-escapes `<`, `>`, `&` (and `U+2028`/`U+2029`),
//! * indents nested values by two spaces with one space after each `:`,
//! * appends **exactly one** trailing `\n`.
//!
//! [`JsonCodec`] reproduces all three via a `serde_json` [`Formatter`] that
//! applies Go's escaping rules (the same `GoCompatFormatter` strategy used by
//! the `cf-persist` crate), so a `checkpoint.json` produced by the Rust build is
//! byte-identical to one produced by Go for the same metadata. The wall-clock
//! `created_at` field is injected by the caller and pinned in goldens
//! (DESIGN §2.8).

use crate::error::{CheckpointError, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter, PrettyFormatter};
use std::io::{self, Read, Write};
use std::path::Path;

/// Default two-space indentation, matching Go's `NewJSONCodec` /
/// `json.MarshalIndent(v, "", "  ")`.
const DEFAULT_INDENT: &str = "  ";

/// A serialization codec for analyzer / checkpoint state.
///
/// Ported from Go's `persist.Codec`:
///
/// ```go
/// type Codec interface {
///     Encode(w io.Writer, v any) error
///     Decode(r io.Reader, v any) error
///     Extension() string
/// }
/// ```
///
/// In Rust the value type is a generic parameter rather than `any`.
pub trait Codec {
    /// Encodes `value` to the writer.
    fn encode<W: Write, T: Serialize>(&self, writer: W, value: &T) -> Result<()>;

    /// Decodes a value of type `T` from the reader.
    fn decode<R: Read, T: DeserializeOwned>(&self, reader: R) -> Result<T>;

    /// Returns the canonical file extension for this codec (including the dot).
    fn extension(&self) -> &'static str;
}

/// JSON codec matching Go's `persist.JSONCodec`.
///
/// ```go
/// type JSONCodec struct { Indent string }
/// ```
///
/// When [`indent`](JsonCodec::indent) is empty the output is compact; otherwise
/// the given string is used per nesting level. Output is always Go-`encoding/json`
/// byte-compatible: HTML escaping on, one space after `:` in pretty mode, and
/// exactly one trailing newline (matching `json.Encoder.Encode`).
#[derive(Debug, Clone, Default)]
pub struct JsonCodec {
    /// Indent string; empty means compact output.
    pub indent: String,
}

impl JsonCodec {
    /// Creates a JSON codec with two-space pretty-printing (Go's `NewJSONCodec`).
    #[must_use]
    pub fn new() -> Self {
        Self {
            indent: DEFAULT_INDENT.to_string(),
        }
    }

    /// Creates a compact JSON codec with no indentation.
    ///
    /// Mirrors the test helper `NewCompactJSONCodec` in `codec_test.go`.
    #[must_use]
    pub fn compact() -> Self {
        Self {
            indent: String::new(),
        }
    }

    /// Serializes `value` to a byte vector using this codec's rules. Exposed so
    /// callers (and tests) can inspect the exact bytes without a writer.
    pub fn to_vec<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.encode(&mut buf, value)?;
        Ok(buf)
    }
}

impl Codec for JsonCodec {
    fn encode<W: Write, T: Serialize>(&self, mut writer: W, value: &T) -> Result<()> {
        let mut buf = Vec::with_capacity(128);
        if self.indent.is_empty() {
            let formatter = GoCompatFormatter::new(CompactFormatter);
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
            value
                .serialize(&mut ser)
                .map_err(|e| CheckpointError::Codec(format!("json encode: {e}")))?;
        } else {
            let pretty = PrettyFormatter::with_indent(self.indent.as_bytes());
            let formatter = GoCompatFormatter::new(pretty);
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
            value
                .serialize(&mut ser)
                .map_err(|e| CheckpointError::Codec(format!("json encode: {e}")))?;
        }
        // json.Encoder.Encode always appends exactly one newline.
        buf.push(b'\n');
        writer.write_all(&buf)?;
        Ok(())
    }

    fn decode<R: Read, T: DeserializeOwned>(&self, mut reader: R) -> Result<T> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        serde_json::from_slice(&buf)
            .map_err(|e| CheckpointError::Codec(format!("json decode: {e}")))
    }

    fn extension(&self) -> &'static str {
        ".json"
    }
}

/// Rust-native binary codec standing in for Go's `persist.GobCodec`.
///
/// Per DESIGN §3 the gob wire format is Go-specific and never user-visible, so
/// it is **not** reproduced byte-for-byte. This codec uses `bincode` for the
/// same role: compact, internal-only state written and read back by the same
/// build. Its [`extension`](Codec::extension) is still `.gob` to keep on-disk
/// file naming aligned with the Go layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct GobCodec;

impl GobCodec {
    /// Creates a new binary codec (Go's `NewGobCodec`).
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Codec for GobCodec {
    fn encode<W: Write, T: Serialize>(&self, mut writer: W, value: &T) -> Result<()> {
        let bytes = bincode::serialize(value).map_err(|e| CheckpointError::Codec(e.to_string()))?;
        writer.write_all(&bytes)?;
        Ok(())
    }

    fn decode<R: Read, T: DeserializeOwned>(&self, mut reader: R) -> Result<T> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        bincode::deserialize(&buf).map_err(|e| CheckpointError::Codec(e.to_string()))
    }

    fn extension(&self) -> &'static str {
        ".gob"
    }
}

/// Saves `value` to `dir/basename.ext` using `codec`, creating `dir` if needed.
///
/// Ported from Go's `persist.SaveState`. The write is **atomic**: data is first
/// written to `dir/basename.ext.tmp`, then renamed over the destination. On a
/// mid-write failure the temp file is removed and the destination is left
/// untouched. (Go's `SaveState` writes directly via `os.Create`; this port
/// hardens it with a temp-then-rename so a crash during the checkpoint write
/// cannot corrupt an existing checkpoint — the whole point of the subsystem.)
pub fn save_state<C: Codec, T: Serialize>(
    dir: &Path,
    basename: &str,
    codec: &C,
    value: &T,
) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| io_ctx("create state dir", e))?;

    let path = dir.join(format!("{basename}{}", codec.extension()));
    let tmp_path = dir.join(format!("{basename}{}.tmp", codec.extension()));

    // Encode into the temp file. On any failure, remove the temp file so a
    // partial write can never be promoted to the destination.
    let encode_result = (|| -> Result<()> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .map_err(|e| io_ctx("create temp state file", e))?;
        codec.encode(file, value)?;
        Ok(())
    })();

    if encode_result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
        return encode_result;
    }

    std::fs::rename(&tmp_path, &path).map_err(|e| io_ctx("rename state file", e))?;
    Ok(())
}

/// Loads a value of type `T` from `dir/basename.ext` using `codec`.
///
/// Ported from Go's `persist.LoadState`.
pub fn load_state<C: Codec, T: DeserializeOwned>(
    dir: &Path,
    basename: &str,
    codec: &C,
) -> Result<T> {
    let path = dir.join(format!("{basename}{}", codec.extension()));
    let file = std::fs::File::open(&path).map_err(|e| io_ctx("open state file", e))?;
    codec.decode(file)
}

/// Handles saving and loading typed state, parameterized by codec and value type.
///
/// Ported from Go's generic `persist.Persister[T]`. The basename and codec are
/// fixed at construction; [`save`](Persister::save) / [`load`](Persister::load)
/// take the directory.
#[derive(Debug, Clone)]
pub struct Persister<C: Codec> {
    basename: String,
    codec: C,
}

impl<C: Codec> Persister<C> {
    /// Creates a persister with the given basename and codec (Go's `NewPersister`).
    pub fn new(basename: impl Into<String>, codec: C) -> Self {
        Self {
            basename: basename.into(),
            codec,
        }
    }

    /// Persists `state` to `dir`.
    pub fn save<T: Serialize>(&self, dir: &Path, state: &T) -> Result<()> {
        save_state(dir, &self.basename, &self.codec, state)
    }

    /// Reads state of type `T` from `dir`.
    pub fn load<T: DeserializeOwned>(&self, dir: &Path) -> Result<T> {
        load_state(dir, &self.basename, &self.codec)
    }
}

/// Wraps an I/O error with the same `op: error` context Go attaches via
/// `fmt.Errorf("op: %w", err)`.
fn io_ctx(op: &str, e: std::io::Error) -> CheckpointError {
    CheckpointError::Codec(format!("{op}: {e}"))
}

/// A `serde_json` [`Formatter`] that applies Go's `encoding/json` escaping rules.
///
/// It wraps any inner formatter (e.g. [`CompactFormatter`] or
/// [`PrettyFormatter`]) and only customizes how string *contents* are written,
/// so structural layout (indentation, separators) is delegated to the inner
/// formatter. Within string contents it additionally escapes `<`, `>`, `&`,
/// `U+2028` and `U+2029`, exactly as Go does with `SetEscapeHTML(true)`.
///
/// This mirrors `cf_persist::json::GoCompatFormatter`; the two should be kept in
/// lockstep until this crate depends on `cf-persist` directly.
#[derive(Debug, Clone)]
struct GoCompatFormatter<F> {
    inner: F,
}

impl<F> GoCompatFormatter<F> {
    fn new(inner: F) -> Self {
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

    // --- everything else delegated verbatim to the inner formatter ---

    #[inline]
    fn write_null<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.write_null(w)
    }
    #[inline]
    fn write_bool<W: ?Sized + io::Write>(&mut self, w: &mut W, v: bool) -> io::Result<()> {
        self.inner.write_bool(w, v)
    }
    #[inline]
    fn write_i8<W: ?Sized + io::Write>(&mut self, w: &mut W, v: i8) -> io::Result<()> {
        self.inner.write_i8(w, v)
    }
    #[inline]
    fn write_i16<W: ?Sized + io::Write>(&mut self, w: &mut W, v: i16) -> io::Result<()> {
        self.inner.write_i16(w, v)
    }
    #[inline]
    fn write_i32<W: ?Sized + io::Write>(&mut self, w: &mut W, v: i32) -> io::Result<()> {
        self.inner.write_i32(w, v)
    }
    #[inline]
    fn write_i64<W: ?Sized + io::Write>(&mut self, w: &mut W, v: i64) -> io::Result<()> {
        self.inner.write_i64(w, v)
    }
    #[inline]
    fn write_i128<W: ?Sized + io::Write>(&mut self, w: &mut W, v: i128) -> io::Result<()> {
        self.inner.write_i128(w, v)
    }
    #[inline]
    fn write_u8<W: ?Sized + io::Write>(&mut self, w: &mut W, v: u8) -> io::Result<()> {
        self.inner.write_u8(w, v)
    }
    #[inline]
    fn write_u16<W: ?Sized + io::Write>(&mut self, w: &mut W, v: u16) -> io::Result<()> {
        self.inner.write_u16(w, v)
    }
    #[inline]
    fn write_u32<W: ?Sized + io::Write>(&mut self, w: &mut W, v: u32) -> io::Result<()> {
        self.inner.write_u32(w, v)
    }
    #[inline]
    fn write_u64<W: ?Sized + io::Write>(&mut self, w: &mut W, v: u64) -> io::Result<()> {
        self.inner.write_u64(w, v)
    }
    #[inline]
    fn write_u128<W: ?Sized + io::Write>(&mut self, w: &mut W, v: u128) -> io::Result<()> {
        self.inner.write_u128(w, v)
    }
    #[inline]
    fn write_f32<W: ?Sized + io::Write>(&mut self, w: &mut W, v: f32) -> io::Result<()> {
        self.inner.write_f32(w, v)
    }
    #[inline]
    fn write_f64<W: ?Sized + io::Write>(&mut self, w: &mut W, v: f64) -> io::Result<()> {
        self.inner.write_f64(w, v)
    }
    #[inline]
    fn write_number_str<W: ?Sized + io::Write>(&mut self, w: &mut W, v: &str) -> io::Result<()> {
        self.inner.write_number_str(w, v)
    }
    #[inline]
    fn begin_string<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.begin_string(w)
    }
    #[inline]
    fn end_string<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.end_string(w)
    }
    #[inline]
    fn write_char_escape<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        e: serde_json::ser::CharEscape,
    ) -> io::Result<()> {
        self.inner.write_char_escape(w, e)
    }
    #[inline]
    fn begin_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.begin_array(w)
    }
    #[inline]
    fn end_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.end_array(w)
    }
    #[inline]
    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.inner.begin_array_value(w, first)
    }
    #[inline]
    fn end_array_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.end_array_value(w)
    }
    #[inline]
    fn begin_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.begin_object(w)
    }
    #[inline]
    fn end_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.end_object(w)
    }
    #[inline]
    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.inner.begin_object_key(w, first)
    }
    #[inline]
    fn end_object_key<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.end_object_key(w)
    }
    #[inline]
    fn begin_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.begin_object_value(w)
    }
    #[inline]
    fn end_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.inner.end_object_value(w)
    }
    #[inline]
    fn write_raw_fragment<W: ?Sized + io::Write>(&mut self, w: &mut W, f: &str) -> io::Result<()> {
        self.inner.write_raw_fragment(w, f)
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
    use std::collections::BTreeMap;
    use std::io::Cursor;

    // Mirrors codec_test.go's `testState`.
    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestState {
        name: String,
        count: i64,
        values: BTreeMap<String, i64>,
    }

    // Ported from TestJSONCodec_RoundTrip.
    #[test]
    fn json_codec_round_trip() {
        let codec = JsonCodec::new();
        let mut values = BTreeMap::new();
        values.insert("a".to_string(), 1);
        values.insert("b".to_string(), 2);
        let original = TestState {
            name: "test".into(),
            count: 42,
            values,
        };

        let mut buf = Vec::new();
        codec.encode(&mut buf, &original).unwrap();

        let decoded: TestState = codec.decode(Cursor::new(buf)).unwrap();
        assert_eq!(decoded.name, original.name);
        assert_eq!(decoded.count, original.count);
        assert_eq!(decoded.values.len(), original.values.len());
    }

    // Ported from TestJSONCodec_Extension.
    #[test]
    fn json_codec_extension() {
        assert_eq!(JsonCodec::new().extension(), ".json");
    }

    // Ported from TestCompactJSONCodec_NoIndent.
    #[test]
    fn compact_json_codec_no_indent() {
        let codec = JsonCodec::compact();
        let state = TestState {
            name: "compact".into(),
            count: 1,
            values: BTreeMap::new(),
        };
        let mut buf = Vec::new();
        codec.encode(&mut buf, &state).unwrap();
        let output = String::from_utf8(buf).unwrap();
        // Compact JSON has at most the single trailing newline.
        assert!(
            output.matches('\n').count() <= 1,
            "compact JSON has too many newlines: {output:?}"
        );
    }

    // Ported from TestGobCodec_RoundTrip.
    #[test]
    fn gob_codec_round_trip() {
        let codec = GobCodec::new();
        let mut values = BTreeMap::new();
        values.insert("x".to_string(), 10);
        values.insert("y".to_string(), 20);
        let original = TestState {
            name: "gob-test".into(),
            count: 123,
            values,
        };
        let mut buf = Vec::new();
        codec.encode(&mut buf, &original).unwrap();
        let decoded: TestState = codec.decode(Cursor::new(buf)).unwrap();
        assert_eq!(decoded.name, original.name);
        assert_eq!(decoded.count, original.count);
    }

    // Ported from TestGobCodec_Extension.
    #[test]
    fn gob_codec_extension() {
        assert_eq!(GobCodec::new().extension(), ".gob");
    }

    #[test]
    fn json_encode_appends_exactly_one_newline() {
        let codec = JsonCodec::compact();
        let bytes = codec
            .to_vec(&TestState {
                name: "x".into(),
                count: 0,
                values: BTreeMap::new(),
            })
            .unwrap();
        assert_eq!(*bytes.last().unwrap(), b'\n');
        assert_ne!(bytes[bytes.len() - 2], b'\n');
    }

    #[test]
    fn json_html_escapes_angle_brackets_and_amp() {
        let codec = JsonCodec::compact();
        let mut values = BTreeMap::new();
        values.insert("k".to_string(), 1);
        let bytes = codec
            .to_vec(&TestState {
                name: "<a> & </b>".into(),
                count: 0,
                values,
            })
            .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("\\u003c"), "expected escaped <: {s}");
        assert!(s.contains("\\u003e"), "expected escaped >: {s}");
        assert!(s.contains("\\u0026"), "expected escaped &: {s}");
        assert!(!s.contains('<'));
        assert!(!s.contains('>'));
        assert!(!s.contains('&'));
    }

    #[test]
    fn json_escapes_line_and_paragraph_separators() {
        let codec = JsonCodec::compact();
        let mut values = BTreeMap::new();
        values.insert("k".to_string(), 1);
        let bytes = codec
            .to_vec(&TestState {
                name: "x\u{2028}y\u{2029}z".into(),
                count: 0,
                values,
            })
            .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("\\u2028"), "{s}");
        assert!(s.contains("\\u2029"), "{s}");
    }

    #[test]
    fn pretty_indent_two_spaces_and_colon_space() {
        let codec = JsonCodec::new();
        let mut values = BTreeMap::new();
        values.insert("a".to_string(), 1);
        let bytes = codec
            .to_vec(&TestState {
                name: "n".into(),
                count: 2,
                values,
            })
            .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        // Pretty output uses "key": value with a space after the colon and a
        // two-space indent for nested entries.
        assert!(s.contains("\"name\": \"n\""), "{s}");
        assert!(s.contains("\n  \"name\""), "expected two-space indent: {s}");
        assert!(s.ends_with("}\n"));
    }

    #[test]
    fn pretty_indent_collapses_empty_containers() {
        let codec = JsonCodec::new();
        // values is an empty map -> should render as {} not {\n}
        let bytes = codec
            .to_vec(&TestState {
                name: "n".into(),
                count: 0,
                values: BTreeMap::new(),
            })
            .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("\"values\": {}"), "empty map not collapsed: {s}");
    }

    #[test]
    fn map_keys_sorted_on_serialize() {
        let codec = JsonCodec::compact();
        let mut values = BTreeMap::new();
        values.insert("b".to_string(), 2);
        values.insert("a".to_string(), 1);
        values.insert("c".to_string(), 3);
        let bytes = codec
            .to_vec(&TestState {
                name: "n".into(),
                count: 0,
                values,
            })
            .unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.find("\"a\"").unwrap() < s.find("\"b\"").unwrap());
        assert!(s.find("\"b\"").unwrap() < s.find("\"c\"").unwrap());
    }

    #[test]
    fn save_state_is_atomic_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let codec = JsonCodec::new();
        let state = TestState {
            name: "persisted".into(),
            count: 7,
            values: BTreeMap::new(),
        };
        save_state(dir.path(), "state", &codec, &state).unwrap();
        // No leftover temp file.
        assert!(!dir.path().join("state.json.tmp").exists());
        assert!(dir.path().join("state.json").exists());

        let loaded: TestState = load_state(dir.path(), "state", &codec).unwrap();
        assert_eq!(loaded, state);
    }

    #[test]
    fn persister_save_load() {
        let dir = tempfile::tempdir().unwrap();
        let p = Persister::new("snap", JsonCodec::new());
        let state = TestState {
            name: "p".into(),
            count: 9,
            values: BTreeMap::new(),
        };
        p.save(dir.path(), &state).unwrap();
        let loaded: TestState = p.load(dir.path()).unwrap();
        assert_eq!(loaded, state);
    }
}

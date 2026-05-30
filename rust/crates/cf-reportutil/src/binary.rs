//! Binary serialization for analyzer reports — the CFB1 envelope.
//!
//! Port of `internal/analyzers/common/reportutil/binary.go`. The "bin" /
//! "binary" machine format is a simple length-prefixed framing format (NOT
//! compression — there is no LZ4 anywhere in codefang, see DESIGN.md §0).
//!
//! Each envelope is laid out as:
//!
//! ```text
//! [magic:4 = "CFB1"][length:4 = u32 little-endian][json-payload:length]
//! ```
//!
//! where `json-payload` is [`cf_gojson::marshal`] output — compact,
//! HTML-escape ON, no trailing newline — exactly mirroring Go's
//! `json.Marshal(value)` (binary.go:31). Multiple envelopes concatenate
//! back-to-back; the decoder loops while bytes remain. See DESIGN.md §2.5.

use cf_gojson::{marshal, GoValue};

/// Maximum payload size, mirroring Go's `math.MaxUint32` bound (binary.go:36).
pub const MAX_PAYLOAD_SIZE: usize = u32::MAX as usize;

/// Identifies a codefang binary envelope. Mirrors `BinaryMagic` (binary.go:17).
pub const BINARY_MAGIC: &[u8; 4] = b"CFB1";

/// Magic (4) + length (4). Mirrors `binaryHeaderSize` (binary.go:19).
pub const BINARY_HEADER_SIZE: usize = 8;

/// Size of the little-endian length field.
pub const BINARY_LENGTH_SIZE: usize = 4;

/// Error encoding a binary envelope.
///
/// Mirrors `ErrBinaryPayloadTooLarge` (binary.go:26). The [`Display`] wording
/// matches Go's `fmt.Errorf("%w: %d bytes", …)` so error output stays identical.
///
/// [`Display`]: std::fmt::Display
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// The marshalled payload exceeds [`MAX_PAYLOAD_SIZE`].
    PayloadTooLarge {
        /// Actual payload length in bytes.
        len: usize,
    },
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Mirrors: "binary payload too large: %d bytes" (binary.go:26,37).
            Self::PayloadTooLarge { len } => {
                write!(f, "binary payload too large: {len} bytes")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// Error decoding a binary envelope stream.
///
/// Mirrors `ErrInvalidBinaryEnvelope` (binary.go:24): Go reports bad magic and
/// truncation through the same sentinel via `errors.Join` / `fmt.Errorf`. Here
/// the distinct cause is preserved as separate variants while keeping the shared
/// "invalid binary envelope" prefix in [`Display`].
///
/// [`Display`]: std::fmt::Display
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// An envelope did not start with the `CFB1` magic (binary.go:66).
    BadMagic,
    /// An envelope claimed more bytes than remain in the buffer (binary.go:73).
    Truncated {
        /// Payload length the header claimed.
        need: usize,
        /// Bytes actually available after the header.
        have: usize,
    },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Mirrors: "invalid binary envelope: bad magic" (binary.go:67).
            Self::BadMagic => write!(f, "invalid binary envelope: bad magic"),
            // Go joins ErrInvalidBinaryEnvelope with the io.ReadFull error
            // (io.ErrUnexpectedEOF) on truncation; the stable prefix is kept.
            Self::Truncated { need, have } => write!(
                f,
                "invalid binary envelope: unexpected EOF (need {need} bytes, have {have})"
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Encodes a value as a length-prefixed CFB1 binary envelope.
///
/// Format: `[magic:4][length:4][json-payload:length]`. This is framing, not
/// compression. The payload is [`cf_gojson::marshal`] — compact, HTML-escape ON,
/// no trailing newline — matching Go's `json.Marshal(value)` exactly
/// (binary.go:31). Mirrors `EncodeBinaryEnvelope` (binary.go:30); Go writes to an
/// `io.Writer`, this port returns the bytes (callers write them where needed).
///
/// # Errors
///
/// Returns [`EncodeError::PayloadTooLarge`] if the payload exceeds
/// [`MAX_PAYLOAD_SIZE`] (mirrors binary.go:36).
///
/// # Examples
///
/// ```
/// use cf_gojson::{GoMap, GoValue, MapOrigin};
/// use cf_reportutil::binary::encode_binary_envelope;
///
/// let mut m = GoMap::new(MapOrigin::Map);
/// m.insert("key", GoValue::Str("value".into()));
/// let bytes = encode_binary_envelope(&GoValue::Map(m)).unwrap();
/// assert_eq!(&bytes[..4], b"CFB1");
/// ```
pub fn encode_binary_envelope(value: &GoValue) -> Result<Vec<u8>, EncodeError> {
    let payload = marshal(value);

    if payload.len() > MAX_PAYLOAD_SIZE {
        return Err(EncodeError::PayloadTooLarge { len: payload.len() });
    }

    let mut buf = Vec::with_capacity(BINARY_HEADER_SIZE + payload.len());
    buf.extend_from_slice(BINARY_MAGIC);
    // binary.LittleEndian.PutUint32 of the payload length (binary.go:42).
    let len = payload.len() as u32;
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(&payload);

    Ok(buf)
}

/// Decodes one CFB1 envelope from the front of `data`.
///
/// On success returns `(payload, rest)` where `payload` is the compact-JSON
/// bytes and `rest` is the remaining input after this envelope. Mirrors
/// `DecodeBinaryEnvelope` (binary.go:58).
///
/// The Go function reads from an `io.Reader` and returns the payload bytes; it
/// does **not** JSON-unmarshal here (the caller does). This port likewise
/// returns the raw payload, because `cf-gojson` ships only the Go-compatible
/// JSON *encoder*, not a parser — preserving byte-exact round-tripping.
///
/// # Errors
///
/// - [`DecodeError::BadMagic`] if the magic prefix is not `CFB1` (binary.go:66).
/// - [`DecodeError::Truncated`] if fewer than `header + length` bytes remain
///   (mirrors the `io.ReadFull` short-read paths, binary.go:61/73).
pub fn decode_binary_envelope(data: &[u8]) -> Result<(&[u8], &[u8]), DecodeError> {
    // io.ReadFull(reader, header) fails if fewer than 8 bytes remain.
    if data.len() < BINARY_HEADER_SIZE {
        return Err(DecodeError::Truncated {
            need: BINARY_HEADER_SIZE,
            have: data.len(),
        });
    }

    if &data[..4] != BINARY_MAGIC {
        return Err(DecodeError::BadMagic);
    }

    let len_bytes: [u8; BINARY_LENGTH_SIZE] = data[4..BINARY_HEADER_SIZE]
        .try_into()
        .expect("slice is exactly BINARY_LENGTH_SIZE bytes");
    let length = u32::from_le_bytes(len_bytes) as usize;

    let payload_start = BINARY_HEADER_SIZE;
    let payload_end = payload_start + length;
    if payload_end > data.len() {
        return Err(DecodeError::Truncated {
            need: length,
            have: data.len() - payload_start,
        });
    }

    Ok((&data[payload_start..payload_end], &data[payload_end..]))
}

/// Decodes all concatenated CFB1 envelopes from `data`.
///
/// Returns the raw compact-JSON payloads in order. Mirrors
/// `DecodeBinaryEnvelopes` (binary.go:82): it loops while bytes remain, decoding
/// one envelope per iteration. Unlike Go (which JSON-unmarshals each payload into
/// `any`), this returns the payload bytes; see [`decode_binary_envelope`] for
/// why.
///
/// # Errors
///
/// Propagates any [`DecodeError`] from [`decode_binary_envelope`]. Note that a
/// trailing partial header (fewer than 8 bytes) is reported as
/// [`DecodeError::Truncated`], matching Go's `io.ReadFull` failure on the next
/// loop iteration (the Go loop condition is `reader.Len() > 0`).
pub fn decode_binary_envelopes(data: &[u8]) -> Result<Vec<&[u8]>, DecodeError> {
    let mut payloads = Vec::new();
    let mut rest = data;

    while !rest.is_empty() {
        let (payload, next) = decode_binary_envelope(rest)?;
        payloads.push(payload);
        rest = next;
    }

    Ok(payloads)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::{GoMap, MapOrigin};

    fn obj(pairs: &[(&str, GoValue)]) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        for (k, v) in pairs {
            m.insert(*k, v.clone());
        }
        GoValue::Map(m)
    }

    // Port of TestEncodeDecodeBinaryEnvelope_RoundTrip (binary_test.go:11).
    #[test]
    fn encode_decode_binary_envelope_round_trip() {
        let value = obj(&[
            ("name", GoValue::Str("test".into())),
            ("value", GoValue::Int(42)),
        ]);

        let encoded = encode_binary_envelope(&value).expect("encode");
        assert!(encoded.len() >= BINARY_HEADER_SIZE);
        assert_eq!(&encoded[..4], BINARY_MAGIC);

        let (payload, rest) = decode_binary_envelope(&encoded).expect("decode");
        assert!(rest.is_empty());
        // map-origin byte-sorts: name < value.
        assert_eq!(payload, br#"{"name":"test","value":42}"#);
    }

    // Port of TestDecodeBinaryEnvelope_InvalidMagic (binary_test.go:34).
    #[test]
    fn decode_binary_envelope_invalid_magic() {
        let data = b"BAD!\x00\x00\x00\x00";
        let err = decode_binary_envelope(data).expect_err("expected bad magic");
        assert_eq!(err, DecodeError::BadMagic);
        assert_eq!(err.to_string(), "invalid binary envelope: bad magic");
    }

    // Port of TestDecodeBinaryEnvelope_Truncated (binary_test.go:42).
    // Header claims 5-byte payload but only one byte follows.
    #[test]
    fn decode_binary_envelope_truncated() {
        let data = [b'C', b'F', b'B', b'1', 0x05, 0x00, 0x00, 0x00, b'a'];
        let err = decode_binary_envelope(&data).expect_err("expected truncated");
        assert_eq!(err, DecodeError::Truncated { need: 5, have: 1 });
    }

    // Port of TestDecodeBinaryEnvelopes (binary_test.go:50).
    #[test]
    fn decode_binary_envelopes_multiple() {
        let mut buf = encode_binary_envelope(&obj(&[("id", GoValue::Str("first".into()))]))
            .expect("encode first");
        buf.extend_from_slice(
            &encode_binary_envelope(&obj(&[("id", GoValue::Str("second".into()))]))
                .expect("encode second"),
        );

        let payloads = decode_binary_envelopes(&buf).expect("decode");
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0], br#"{"id":"first"}"#);
        assert_eq!(payloads[1], br#"{"id":"second"}"#);
    }

    // Port of TestDecodeBinaryEnvelopes_InvalidPayload (binary_test.go:70).
    // "bad" is 3 bytes — shorter than a header → error (Go's io.ReadFull fails).
    #[test]
    fn decode_binary_envelopes_invalid_payload() {
        let err = decode_binary_envelopes(b"bad").expect_err("expected error");
        assert_eq!(err, DecodeError::Truncated { need: 8, have: 3 });
    }

    // The exact 8-byte header layout: magic + LE u32 length.
    #[test]
    fn header_layout_is_magic_plus_le_u32_length() {
        // payload {"k":"v"} is 9 bytes.
        let encoded =
            encode_binary_envelope(&obj(&[("k", GoValue::Str("v".into()))])).expect("encode");
        assert_eq!(&encoded[..4], b"CFB1");
        assert_eq!(&encoded[4..8], &9u32.to_le_bytes());
        assert_eq!(&encoded[8..], br#"{"k":"v"}"#);
    }

    // Empty input decodes to zero envelopes (Go loop condition reader.Len() > 0).
    #[test]
    fn decode_binary_envelopes_empty() {
        let payloads = decode_binary_envelopes(&[]).expect("decode empty");
        assert!(payloads.is_empty());
    }
}

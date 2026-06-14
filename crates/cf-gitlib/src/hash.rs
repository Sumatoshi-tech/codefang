//! Git object hashes (SHA-1).
//!
//! [`Hash`] is a 20-byte SHA-1 digest: hex parsing ([`Hash::new`]), lowercase
//! hex rendering ([`Hash::to_hex`] via [`std::fmt::Display`]), a zero check
//! ([`Hash::is_zero`]), and lossless conversion to/from a libgit2
//! [`git2::Oid`].
//!
//! # Byte-identity
//!
//! [`Hash::to_hex`] renders a 40-character, lowercase, fixed-width hex string.
//! Hashes surface into machine reports as these strings, so the rendering is
//! part of the report contract (pinned by `tests/compat`).
//!
//! ```
//! use cf_gitlib::Hash;
//!
//! let h = Hash::new("0123456789ABCDEF0123456789abcdef01234567");
//! // Rendering is always 40 lowercase hex chars, regardless of input case.
//! assert_eq!(h.to_string(), "0123456789abcdef0123456789abcdef01234567");
//! assert!(!h.is_zero());
//! assert!(Hash::zero().is_zero());
//! ```

use std::fmt;

/// Size of a SHA-1 hash in bytes.
pub const HASH_SIZE: usize = 20;

/// Size of a hex-encoded SHA-1 hash.
pub const HASH_HEX_SIZE: usize = 40;

/// Base offset for hex digits `a`..`f`.
const HEX_BASE: u8 = 10;

/// Bit shift for the high nibble of a byte.
const HEX_SHIFT: u8 = 4;

/// A git object hash (SHA-1), a fixed 20-byte array.
///
/// A fixed 20-byte array; cheap to copy and compare by value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash(pub [u8; HASH_SIZE]);

impl Hash {
    /// The zero-value hash (all bytes `0x00`), as an associated constant.
    ///
    /// Provided as a `const` so it can be used directly in struct-field
    /// initializers.
    pub const ZERO: Hash = Hash([0u8; HASH_SIZE]);

    /// Returns the zero-value hash (all bytes `0x00`).
    #[must_use]
    pub const fn zero() -> Self {
        Hash::ZERO
    }

    /// Parses a [`Hash`] from a hex string.
    ///
    /// Reads up to [`HASH_SIZE`] bytes from the string, accepting upper- or
    /// lower-case hex digits, and leaves any not-yet-written bytes as zero.
    /// Invalid characters decode to nibble `0` (reference-implementation
    /// behavior). A string shorter than 40 chars fills only the leading bytes;
    /// the rest stay zero.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cf_gitlib::Hash;
    /// let h = Hash::new("0123456789abcdef0123456789abcdef01234567");
    /// assert_eq!(h.to_string(), "0123456789abcdef0123456789abcdef01234567");
    /// assert_eq!(Hash::new("abcd").0[0], 0xab);
    /// ```
    #[must_use]
    pub fn new(hex_str: &str) -> Self {
        let bytes = hex_str.as_bytes();
        let mut hash = [0u8; HASH_SIZE];

        let mut i = 0;
        while i < HASH_SIZE && i * 2 + 1 < bytes.len() {
            let c1 = bytes[i * 2];
            let c2 = bytes[i * 2 + 1];
            hash[i] = (hex_char_to_nibble(c1) << HEX_SHIFT) | hex_char_to_nibble(c2);
            i += 1;
        }

        Hash(hash)
    }

    /// Builds a [`Hash`] from a libgit2 [`git2::Oid`].
    #[must_use]
    pub fn from_oid(oid: &git2::Oid) -> Self {
        let mut h = [0u8; HASH_SIZE];
        // libgit2 OIDs are 20 bytes for SHA-1; copy the leading HASH_SIZE bytes.
        let raw = oid.as_bytes();
        let n = raw.len().min(HASH_SIZE);
        h[..n].copy_from_slice(&raw[..n]);
        Hash(h)
    }

    /// Converts this hash to a libgit2 [`git2::Oid`].
    ///
    /// [`Hash::from_oid`] and `to_oid` round-trip losslessly:
    ///
    /// ```
    /// # use cf_gitlib::Hash;
    /// let h = Hash::new("0123456789abcdef0123456789abcdef01234567");
    /// assert_eq!(Hash::from_oid(&h.to_oid()), h);
    /// ```
    ///
    /// # Panics
    ///
    /// Never panics for a valid 20-byte SHA-1: [`git2::Oid::from_bytes`] only
    /// fails when the slice length is not a supported OID size, and [`HASH_SIZE`]
    /// is always exactly 20.
    #[must_use]
    pub fn to_oid(&self) -> git2::Oid {
        git2::Oid::from_bytes(&self.0).expect("20-byte slice is always a valid SHA-1 OID")
    }

    /// Returns the lowercase 40-character hex representation.
    ///
    /// # Examples
    ///
    /// ```
    /// # use cf_gitlib::Hash;
    /// assert_eq!(Hash::zero().to_hex(), "0".repeat(40));
    /// assert_eq!(
    ///     Hash::new("abcdef0123456789abcdef0123456789abcdef01").to_hex(),
    ///     "abcdef0123456789abcdef0123456789abcdef01",
    /// );
    /// ```
    ///
    /// # Panics
    ///
    /// Never panics in practice: every produced byte is an ASCII hex digit, so
    /// the UTF-8 conversion always succeeds.
    #[must_use]
    pub fn to_hex(&self) -> String {
        const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
        let mut buf = [0u8; HASH_HEX_SIZE];
        for (i, &byte_val) in self.0.iter().enumerate() {
            buf[i * 2] = HEX_CHARS[(byte_val >> HEX_SHIFT) as usize];
            buf[i * 2 + 1] = HEX_CHARS[(byte_val & 0x0f) as usize];
        }
        // SAFETY of correctness: every byte is an ASCII hex digit, valid UTF-8.
        String::from_utf8(buf.to_vec()).expect("hex digits are valid UTF-8")
    }

    /// Reports whether every byte is zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

/// Converts a hex character to its 4-bit value.
///
/// Unrecognized characters map to `0` (reference-implementation behavior).
const fn hex_char_to_nibble(char: u8) -> u8 {
    match char {
        b'0'..=b'9' => char - b'0',
        b'a'..=b'f' => char - b'a' + HEX_BASE,
        b'A'..=b'F' => char - b'A' + HEX_BASE,
        _ => 0,
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.to_hex())
    }
}

impl Default for Hash {
    fn default() -> Self {
        Hash::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors reference test TestZeroHash.
    #[test]
    fn zero_hash() {
        let hash = Hash::zero();
        assert_eq!(hash, Hash([0u8; HASH_SIZE]));
        assert!(hash.is_zero());
    }

    // Mirrors reference test TestNewHash. (table-driven).
    #[test]
    fn new_hash_cases() {
        let full = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
        ];
        assert_eq!(
            Hash::new("0123456789abcdef0123456789abcdef01234567"),
            Hash(full)
        );
        // Upper-case hex decodes identically.
        assert_eq!(
            Hash::new("0123456789ABCDEF0123456789ABCDEF01234567"),
            Hash(full)
        );
        // All zeros.
        assert_eq!(
            Hash::new("0000000000000000000000000000000000000000"),
            Hash([0u8; HASH_SIZE])
        );
        // All f's.
        assert_eq!(
            Hash::new("ffffffffffffffffffffffffffffffffffffffff"),
            Hash([0xff; HASH_SIZE])
        );
        // Short string fills only the leading bytes.
        let mut short = [0u8; HASH_SIZE];
        short[0] = 0xab;
        short[1] = 0xcd;
        assert_eq!(Hash::new("abcd"), Hash(short));
        // Empty string => zero hash.
        assert_eq!(Hash::new(""), Hash([0u8; HASH_SIZE]));
    }

    // Mirrors reference test TestHashString.
    #[test]
    fn hash_string_cases() {
        assert_eq!(
            Hash::zero().to_string(),
            "0000000000000000000000000000000000000000"
        );
        assert_eq!(
            Hash([0xff; HASH_SIZE]).to_string(),
            "ffffffffffffffffffffffffffffffffffffffff"
        );
        let mixed = Hash([
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
            0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
        ]);
        assert_eq!(mixed.to_string(), "0123456789abcdef0123456789abcdef01234567");
    }

    // Mirrors reference test TestHashIsZero.
    #[test]
    fn hash_is_zero_cases() {
        assert!(Hash::zero().is_zero());
        let mut first = [0u8; HASH_SIZE];
        first[0] = 0x01;
        assert!(!Hash(first).is_zero());
        let mut last = [0u8; HASH_SIZE];
        last[HASH_SIZE - 1] = 0x01;
        assert!(!Hash(last).is_zero());
    }

    // Mirrors reference test TestHashRoundTrip.
    #[test]
    fn hash_round_trip() {
        let original = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(Hash::new(original).to_string(), original);
    }

    // Mirrors reference test TestHashFromOid. / TestHashToOid / TestHashOidRoundTrip.
    #[test]
    fn hash_oid_round_trip() {
        let bytes = [
            0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
            0xde, 0xf0, 0xfe, 0xdc, 0xba, 0x98,
        ];
        let oid = git2::Oid::from_bytes(&bytes).unwrap();
        let hash = Hash::from_oid(&oid);
        assert_eq!(hash.0, bytes);
        assert_eq!(hash.to_oid(), oid);
    }

    // Mirrors reference test TestHashConstants. (gitlib.HashSize / HashHexSize).
    #[test]
    fn hash_constants() {
        assert_eq!(HASH_SIZE, 20);
        assert_eq!(HASH_HEX_SIZE, 40);
    }
}

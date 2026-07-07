//! Minimal git object types consumed by the blob cache.
//!
//! The fixed-size [`GitHash`] and the [`CachedBlob`] payload are the only git
//! value types the cache needs.
//!
//! # Replacement note
//!
//! The full workspace crate `cf-gitlib` (the libgit2 access layer) is currently
//! an unfinished scaffold. `cf-cache` only needs these two value types, so they
//! are defined locally here to keep the crate compiling and testable in
//! isolation. Once `cf-gitlib` is complete, re-export `Hash` and `CachedBlob`
//! from it and delete this module.

/// Lowercase hex digits, indexed by nibble value.
pub(crate) const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Number of bytes in a Git object hash (SHA-1 = 20 bytes).
pub const HASH_SIZE: usize = 20;

/// A Git object hash as a fixed-size byte array.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GitHash(pub [u8; HASH_SIZE]);

impl GitHash {
    /// Returns the underlying bytes as a slice (used by the cache for
    /// Bloom-filter membership).
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Returns the lowercase hex-encoded representation.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(HASH_SIZE * 2);
        for b in self.0 {
            s.push(HEX_DIGITS[(b >> 4) as usize] as char);
            s.push(HEX_DIGITS[(b & 0x0f) as usize] as char);
        }
        s
    }
}

impl std::fmt::Display for GitHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl From<[u8; HASH_SIZE]> for GitHash {
    fn from(bytes: [u8; HASH_SIZE]) -> Self {
        Self(bytes)
    }
}

/// A Git blob with its hash and content.
///
/// The reference implementation's blob type also caches a line count and keeps
/// an mmap keep-alive handle; neither is part of the cache's value semantics
/// (a clone copies nothing observable beyond the data + hash), so only `data`
/// and `hash` are kept here.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CachedBlob {
    /// Raw blob content.
    pub data: Vec<u8>,
    /// Blob object hash (zero-valued when constructed without one).
    pub hash: GitHash,
}

impl CachedBlob {
    /// Creates a blob from data only, leaving the hash zero-valued.
    #[must_use]
    pub fn for_test(data: Vec<u8>) -> Self {
        Self {
            data,
            hash: GitHash::default(),
        }
    }

    /// Creates a blob with an explicit hash.
    #[must_use]
    pub const fn with_hash_for_test(hash: GitHash, data: Vec<u8>) -> Self {
        Self { data, hash }
    }

    /// Returns a detached deep copy.
    ///
    /// This is the function the LRU's clone option calls so that arena-backed
    /// blob data is detached before being stored in the cache.
    #[must_use]
    pub fn clone_blob(&self) -> Self {
        Self {
            data: self.data.clone(),
            hash: self.hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encoding() {
        let mut h = GitHash::default();
        h.0[0] = 0xab;
        h.0[1] = 0x0f;
        let s = h.to_hex();
        assert!(s.starts_with("ab0f"));
        assert_eq!(s.len(), HASH_SIZE * 2);
    }

    #[test]
    fn clone_is_deep() {
        let b = CachedBlob::for_test(b"abc".to_vec());
        let c = b.clone_blob();
        assert_eq!(b, c);
        assert_eq!(c.data, b"abc");
    }

    #[test]
    fn as_bytes_matches_array() {
        let mut h = GitHash::default();
        h.0[5] = 0x42;
        assert_eq!(h.as_bytes().len(), HASH_SIZE);
        assert_eq!(h.as_bytes()[5], 0x42);
    }
}

//! The commit hash a typo fix is attributed to.

use std::fmt;

/// Length of a Git SHA-1 hash in bytes.
pub const HASH_SIZE: usize = 20;

/// A Git object hash (SHA-1), a 20-byte array.
///
/// Mirrors `cf_gitlib::Hash` so the crate stays free of the git dependency;
/// the history walk converts at the boundary (`Hash(hash.0)`). The zero value
/// is all-zero bytes; [`Hash::string`] renders lowercase hex.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct Hash(pub [u8; HASH_SIZE]);

impl Hash {
    /// Returns the lowercase hex string representation.
    #[must_use]
    pub fn string(&self) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(HASH_SIZE * 2);
        for b in &self.0 {
            let _ = write!(s, "{b:02x}");
        }
        s
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_hex_zero() {
        assert_eq!(Hash::default().string(), "0".repeat(40));
    }

    #[test]
    fn hash_hex_nonzero() {
        let mut b = [0u8; 20];
        b[0] = 0xab;
        b[19] = 0x0f;
        let s = Hash(b).string();
        assert!(s.starts_with("ab"));
        assert!(s.ends_with("0f"));
        assert_eq!(s.len(), 40);
        assert_eq!(Hash(b).to_string(), s);
    }
}

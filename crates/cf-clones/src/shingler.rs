//! K-gram shingling of a function's pre-order node-type sequence.
//!
//! A *shingle* is a window of `k` consecutive node types joined by
//! [`SHINGLE_SEPARATOR`]. Each shingle is hashed with FNV-1a; the resulting
//! sorted, deduplicated set is both the input of the `MinHash` signature and
//! the exact set the engine verifies candidate pairs against.

use cf_alg_hashutil::fnv64a;

/// Default k-gram window size for shingling.
pub const DEFAULT_SHINGLE_SIZE: usize = 5;

/// Separator placed between node types within a shingle.
pub const SHINGLE_SEPARATOR: &str = "|";

/// Extracts k-gram shingles from a node-type sequence.
#[derive(Debug, Clone, Copy)]
pub struct Shingler {
    k: usize,
}

impl Default for Shingler {
    fn default() -> Self {
        Self::new(DEFAULT_SHINGLE_SIZE)
    }
}

impl Shingler {
    /// Creates a new shingler with the given k-gram size.
    #[must_use]
    pub fn new(k: usize) -> Self {
        Self { k }
    }

    /// The configured k-gram window size.
    #[must_use]
    pub fn k(&self) -> usize {
        self.k
    }

    /// Returns the sorted, deduplicated FNV-1a hashes of every `k`-gram of
    /// `types` (each window joined by [`SHINGLE_SEPARATOR`]). Empty when the
    /// sequence is shorter than `k`.
    ///
    /// ```
    /// use cf_alg_hashutil::fnv64a;
    /// use cf_clones::Shingler;
    ///
    /// let hashes = Shingler::new(2).shingle_hashes(&["A", "B", "C"]);
    /// let mut want = vec![fnv64a(b"A|B"), fnv64a(b"B|C")];
    /// want.sort_unstable();
    /// assert_eq!(hashes, want);
    ///
    /// // Fewer than k types -> no shingles.
    /// assert!(Shingler::new(5).shingle_hashes(&["A", "B", "C"]).is_empty());
    /// ```
    #[must_use]
    pub fn shingle_hashes(&self, types: &[&str]) -> Vec<u64> {
        if self.k == 0 || types.len() < self.k {
            return Vec::new();
        }
        let mut buf: Vec<u8> = Vec::new();
        let mut hashes: Vec<u64> = types
            .windows(self.k)
            .map(|window| {
                buf.clear();
                for (i, t) in window.iter().enumerate() {
                    if i > 0 {
                        buf.extend_from_slice(SHINGLE_SEPARATOR.as_bytes());
                    }
                    buf.extend_from_slice(t.as_bytes());
                }
                fnv64a(&buf)
            })
            .collect();
        hashes.sort_unstable();
        hashes.dedup();
        hashes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_k_is_five() {
        assert_eq!(Shingler::default().k(), DEFAULT_SHINGLE_SIZE);
        assert_eq!(DEFAULT_SHINGLE_SIZE, 5);
    }

    #[test]
    fn fewer_than_k_types_yields_no_shingles() {
        assert!(Shingler::new(5).shingle_hashes(&["A", "B", "C"]).is_empty());
        assert!(Shingler::new(0).shingle_hashes(&["A"]).is_empty());
    }

    #[test]
    fn shingles_are_k_grams_joined_by_pipe() {
        let hashes = Shingler::new(2).shingle_hashes(&["A", "B", "C"]);
        let mut want = vec![fnv64a(b"A|B"), fnv64a(b"B|C")];
        want.sort_unstable();
        assert_eq!(hashes, want);
    }

    #[test]
    fn repeated_windows_are_a_set() {
        // A|B occurs twice but the set holds it once.
        let hashes = Shingler::new(2).shingle_hashes(&["A", "B", "A", "B"]);
        let mut want = vec![fnv64a(b"A|B"), fnv64a(b"B|A")];
        want.sort_unstable();
        assert_eq!(hashes, want);
    }
}

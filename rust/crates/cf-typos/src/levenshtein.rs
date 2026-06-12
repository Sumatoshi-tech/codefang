//! Self-contained Levenshtein edit distance over Unicode scalar values.
//!
//! Mirrors `cf_alg_levenshtein::Context::distance`, which measures distance in
//! **scalar-value** edits (insert/delete/substitute), each cost 1. The typo
//! detector reuses a [`Context`] for amortized allocation (reusable-row
//! design).
//!
//! Replacing this module with a dependency on `cf-alg-levenshtein` is
//! mechanical (the `Context::distance(&str, &str) -> usize` signature matches).

/// A reusable Levenshtein computation context.
#[derive(Debug, Default, Clone)]
pub struct Context {
    row: Vec<usize>,
}

impl Context {
    /// Creates a new, empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Computes the Levenshtein distance between `a` and `b`.
    ///
    /// Standard two-row dynamic-programming Levenshtein, operating on Unicode
    /// scalar values (`char`), not bytes.
    pub fn distance(&mut self, a: &str, b: &str) -> usize {
        let ra: Vec<char> = a.chars().collect();
        let rb: Vec<char> = b.chars().collect();

        if ra.is_empty() {
            return rb.len();
        }
        if rb.is_empty() {
            return ra.len();
        }

        // row[j] = distance between ra[..i] and rb[..j].
        self.row.clear();
        self.row.extend(0..=rb.len());

        for (i, &ca) in ra.iter().enumerate() {
            let mut prev_diag = self.row[0]; // row[0] before this iteration = i
            self.row[0] = i + 1;
            for (j, &cb) in rb.iter().enumerate() {
                let cost = usize::from(ca != cb);
                let insertion = self.row[j + 1] + 1;
                let deletion = self.row[j] + 1;
                let substitution = prev_diag + cost;
                prev_diag = self.row[j + 1];
                self.row[j + 1] = insertion.min(deletion).min(substitution);
            }
        }

        self.row[rb.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_distances() {
        let mut c = Context::new();
        assert_eq!(c.distance("", ""), 0);
        assert_eq!(c.distance("a", ""), 1);
        assert_eq!(c.distance("", "abc"), 3);
        assert_eq!(c.distance("kitten", "sitting"), 3);
        assert_eq!(c.distance("recieve", "receive"), 2); // transposition = 2 edits
        assert_eq!(c.distance("flaw", "lawn"), 2);
    }

    #[test]
    fn rune_based_not_byte_based() {
        let mut c = Context::new();
        // Single multi-byte char replaced by another single char = distance 1.
        assert_eq!(c.distance("é", "e"), 1);
        assert_eq!(c.distance("café", "cafe"), 1);
    }

    #[test]
    fn reuse_is_consistent() {
        let mut c = Context::new();
        assert_eq!(c.distance("abc", "abd"), 1);
        assert_eq!(c.distance("kitten", "sitting"), 3);
        assert_eq!(c.distance("abc", "abd"), 1);
    }
}

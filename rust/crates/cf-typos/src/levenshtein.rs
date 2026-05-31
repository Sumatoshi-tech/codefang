//! Self-contained Levenshtein edit distance over Unicode scalar values.
//!
//! Mirrors `cf_alg_levenshtein::Context::distance` (Go `pkg/alg/levenshtein`),
//! which measures distance in **rune** edits (insert/delete/substitute), each
//! cost 1. The Go typos analyzer reuses a `levenshtein.Context` for amortized
//! allocation; this [`Context`] keeps the same reusable-row design.
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
    pub fn new() -> Self {
        Context::default()
    }

    /// Computes the Levenshtein distance between `a` and `b` over runes.
    ///
    /// Standard two-row dynamic-programming Levenshtein, operating on Unicode
    /// scalar values (Rust `char`) to match Go's rune semantics.
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

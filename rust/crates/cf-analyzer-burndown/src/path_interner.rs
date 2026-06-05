//! Path interner.
//!
//! Ported from `internal/analyzers/burndown/path_interner.go`. Burndown tracks a
//! large number of file paths; rather than storing the full string on every
//! tracked file/line, paths are interned to small stable integer ids
//! ([`PathId`], Go `PathID = uint32`) so slice-backed state can use the id as an
//! index. The mapping is **insertion-ordered and deterministic**: the first time
//! a path is seen it is assigned the next id (`0, 1, 2, …`), so ids are
//! reproducible across runs given the same commit-walk order.
//!
//! The Go type is mutex-guarded for concurrent use by shard workers. Concurrency
//! discipline in Rust is the caller's choice (wrap in a `Mutex` if shared across
//! threads); the data model and id-assignment semantics are identical.

use std::collections::HashMap;

/// A stable numeric id for an interned path. Mirrors Go `PathID = uint32`.
pub type PathId = u32;

/// Interns file paths to dense, stable [`PathId`]s in first-seen order.
///
/// Mirrors Go `PathInterner` (`Intern` / `Lookup` / `Len`).
#[derive(Clone, Debug, Default)]
pub struct PathInterner {
    ids: HashMap<String, PathId>,
    rev: Vec<String>,
}

impl PathInterner {
    /// Create an empty interner. Mirrors `NewPathInterner`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the [`PathId`] for `path`, creating a new id if `path` has not
    /// been seen. Idempotent: the same path always maps to the same id for the
    /// lifetime of the interner. Mirrors `Intern`.
    pub fn intern(&mut self, path: &str) -> PathId {
        if let Some(&id) = self.ids.get(path) {
            return id;
        }
        let id = PathId::try_from(self.rev.len()).expect("PathID overflow (>u32::MAX paths)");
        self.rev.push(path.to_owned());
        self.ids.insert(path.to_owned(), id);
        id
    }

    /// Returns the path string for `id`. Mirrors `Lookup`.
    ///
    /// # Panics
    ///
    /// Panics if `id >= self.len()`, mirroring Go's `panic("PathID out of
    /// range")`.
    #[must_use]
    pub fn lookup(&self, id: PathId) -> &str {
        self.rev
            .get(id as usize)
            .map(String::as_str)
            .expect("PathID out of range")
    }

    /// Returns the [`PathId`] of an already-interned path, if present.
    /// (Convenience not present in Go; non-mutating lookup.)
    #[must_use]
    pub fn id(&self, path: &str) -> Option<PathId> {
        self.ids.get(path).copied()
    }

    /// Number of interned paths; the next [`PathInterner::intern`] of a new path
    /// returns this value. Mirrors `Len`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rev.len()
    }

    /// Whether no paths have been interned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rev.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of TestPathInterner_InternLookup (path_interner_test.go:8).
    #[test]
    fn intern_lookup() {
        let mut pi = PathInterner::new();
        let id0 = pi.intern("a.go");
        let id1 = pi.intern("b.go");
        assert_ne!(id0, id1, "different paths must get different ids");
        assert_eq!(pi.lookup(id0), "a.go");
        assert_eq!(pi.lookup(id1), "b.go");
        assert_eq!(pi.intern("a.go"), id0, "re-intern returns the same id");
    }

    // Port of TestPathInterner_Len (path_interner_test.go:35).
    #[test]
    fn len_tracks_distinct_paths() {
        let mut pi = PathInterner::new();
        assert_eq!(pi.len(), 0);
        pi.intern("x");
        assert_eq!(pi.len(), 1);
        pi.intern("y");
        assert_eq!(pi.len(), 2);
        pi.intern("x");
        assert_eq!(pi.len(), 2, "re-intern does not grow len");
    }

    #[test]
    fn ids_are_assigned_sequentially_from_zero() {
        let mut pi = PathInterner::new();
        assert_eq!(pi.intern("a"), 0);
        assert_eq!(pi.intern("b"), 1);
        assert_eq!(pi.intern("c"), 2);
    }

    #[test]
    fn id_non_mutating_lookup() {
        let mut pi = PathInterner::new();
        let id = pi.intern("src/main.rs");
        assert_eq!(pi.id("src/main.rs"), Some(id));
        assert_eq!(pi.id("missing"), None);
    }

    #[test]
    #[should_panic(expected = "PathID out of range")]
    fn lookup_out_of_range_panics() {
        let pi = PathInterner::new();
        let _ = pi.lookup(0);
    }

    #[test]
    fn empty_interner() {
        let pi = PathInterner::new();
        assert!(pi.is_empty());
        assert_eq!(pi.len(), 0);
    }
}

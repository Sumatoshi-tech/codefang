//! Native tree-sitter S-expression query/capture compiler and matcher.
//!
//! Tree-sitter provides the underlying `Query`/`QueryCursor` primitives but
//! **not** a cached, pooled DSL-pattern compiler, so this layer adds it
//! (DESIGN.md §5):
//!
//! - [`PatternMatcher::compile_and_cache`] compiles an S-expression pattern to
//!   a tree-sitter [`Query`] and memoizes it by pattern text, tracking
//!   hit/miss counters.
//! - [`PatternMatcher::match_pattern`] runs a compiled query against a node
//!   and returns the captures of the **first** match.
//!
//! Cursors are recycled through a small mutex-guarded free list; the cache is
//! a mutex-guarded map sharing compiled queries via `Arc<Query>`.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use tree_sitter::{Language, Node, Query, QueryCursor};

/// Errors produced by the pattern matcher.
///
/// The error strings are part of the CLI compatibility contract.
#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    /// The query or node argument was missing.
    #[error("query or node is nil")]
    NilQueryArg,
    /// No match was found.
    #[error("no match found")]
    NoMatch,
    /// Tree-sitter query compilation failed.
    #[error("tree-sitter query compilation failed: {0}")]
    Compilation(#[source] tree_sitter::QueryError),
}

/// Compiles and matches S-expression patterns to tree-sitter queries, with a
/// pattern→query cache and a cursor free list.
pub struct PatternMatcher {
    lang: Language,
    inner: Mutex<Inner>,
    cursor_pool: Mutex<Vec<QueryCursor>>,
}

struct Inner {
    cache: HashMap<String, Arc<Query>>,
    hits: i64,
    misses: i64,
}

impl PatternMatcher {
    /// Creates a new matcher for `lang` with an empty cache.
    #[must_use]
    pub fn new(lang: Language) -> Self {
        Self {
            lang,
            inner: Mutex::new(Inner {
                cache: HashMap::new(),
                hits: 0,
                misses: 0,
            }),
            cursor_pool: Mutex::new(Vec::new()),
        }
    }

    /// Compiles a pattern and caches the result, returning the shared compiled
    /// query. On a cache hit the hit counter is bumped; on a miss the pattern
    /// is compiled, stored, and the miss counter is bumped.
    ///
    /// # Errors
    ///
    /// Returns [`MatchError::Compilation`] when the pattern is not a valid
    /// tree-sitter query for the matcher's language.
    pub fn compile_and_cache(&self, pattern: &str) -> Result<Arc<Query>, MatchError> {
        {
            let mut inner = self.inner.lock().expect("pattern matcher cache poisoned");
            if let Some(cached) = inner.cache.get(pattern).map(Arc::clone) {
                inner.hits += 1;
                return Ok(cached);
            }
        }

        let compiled = Arc::new(
            Query::new(&self.lang, pattern).map_err(MatchError::Compilation)?,
        );

        let mut inner = self.inner.lock().expect("pattern matcher cache poisoned");
        // Another thread may have compiled it meanwhile; keep the first stored
        // value (the queries are equivalent, so the choice is not observable).
        let entry = inner
            .cache
            .entry(pattern.to_string())
            .or_insert_with(|| Arc::clone(&compiled));
        let result = Arc::clone(entry);
        inner.misses += 1;
        Ok(result)
    }

    /// Returns the number of cache hits and misses.
    pub fn cache_stats(&self) -> (i64, i64) {
        let inner = self.inner.lock().expect("pattern matcher cache poisoned");
        (inner.hits, inner.misses)
    }

    /// Matches a compiled query against a node and returns the captures of the
    /// first match as a name→text map (a [`BTreeMap`] for deterministic
    /// ordering).
    ///
    /// # Errors
    ///
    /// Returns [`MatchError::NoMatch`] when the query produces no match.
    pub fn match_pattern(
        &self,
        query: &Query,
        node: Node<'_>,
        source: &[u8],
    ) -> Result<BTreeMap<String, String>, MatchError> {
        let mut cursor = self
            .cursor_pool
            .lock()
            .expect("cursor pool poisoned")
            .pop()
            .unwrap_or_default();

        let result = match_tree_sitter_query(query, &mut cursor, node, source);

        // Return the cursor to the pool for reuse.
        self.cursor_pool
            .lock()
            .expect("cursor pool poisoned")
            .push(cursor);

        result
    }
}

/// Takes the first match's captures as a name→text map.
fn match_tree_sitter_query(
    query: &Query,
    cursor: &mut QueryCursor,
    node: Node<'_>,
    source: &[u8],
) -> Result<BTreeMap<String, String>, MatchError> {
    let capture_names = query.capture_names();
    let mut matches = cursor.matches(query, node, source);

    let first = match matches.next() {
        Some(m) => m,
        None => return Err(MatchError::NoMatch),
    };

    let mut captures = BTreeMap::new();
    for cap in first.captures {
        let name = capture_names
            .get(cap.index as usize)
            .copied()
            .unwrap_or("");
        // tree-sitter 0.22 query-match capture nodes are always present, so
        // capture handling reduces to extracting the captured text.
        // `utf8_text` errors only on invalid UTF-8, which this path never
        // receives in practice; skipping on error preserves behavior.
        if let Ok(text) = cap.node.utf8_text(source) {
            captures.insert(name.to_string(), text.to_string());
        }
    }
    Ok(captures)
}

#[cfg(test)]
mod tests {
    // Functional matching tests require a concrete tree-sitter `Language`
    // (a grammar crate), which is not a dependency of `cf-uast-mapping` — the
    // grammar set lives in `cf-uast` (DESIGN.md §5). Those end-to-end tests live
    // there. Here we only assert the error-display wording, which is part of
    // the observable CLI surface.
    use super::*;

    #[test]
    fn error_messages_are_frozen() {
        assert_eq!(MatchError::NilQueryArg.to_string(), "query or node is nil");
        assert_eq!(MatchError::NoMatch.to_string(), "no match found");
    }
}

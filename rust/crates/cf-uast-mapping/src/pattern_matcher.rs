//! Native tree-sitter S-expression query/capture compiler and matcher.
//!
//! Port of Go `pkg/uast/pkg/mapping/pattern_matcher.go`. Tree-sitter provides
//! the underlying `Query`/`QueryCursor` primitives but **not** a cached,
//! pooled DSL-pattern compiler, so this layer is reimplemented (DESIGN.md §5).
//!
//! Behavioral parity with the Go implementation:
//! - [`PatternMatcher::compile_and_cache`] compiles an S-expression pattern to a
//!   tree-sitter [`Query`] and memoizes it by pattern text, tracking hit/miss
//!   counters (Go `CompileAndCache` + `CacheStats`).
//! - [`PatternMatcher::match_pattern`] runs a compiled query against a node and
//!   returns the captures of the **first** match (Go `MatchPattern`).
//!
//! The Go `sync.Pool` of cursors becomes a small mutex-guarded free list; the
//! `sync.RWMutex`-guarded cache becomes a mutex-guarded map. Compiled queries are
//! shared via `Arc<Query>` (Go shared `*sitter.Query` pointers).
//!
//! Available only when the `pattern-matcher` feature is enabled (the default).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use tree_sitter::{Language, Node, Query, QueryCursor};

/// Errors produced by the pattern matcher, mirroring the Go sentinel errors.
#[derive(Debug)]
pub enum MatchError {
    /// The query or node argument was missing (`errNilQueryArg`).
    NilQueryArg,
    /// No match was found (`errNoMatch`).
    NoMatch,
    /// Tree-sitter query compilation failed (`tree-sitter query compilation failed`).
    Compilation(tree_sitter::QueryError),
}

impl std::fmt::Display for MatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchError::NilQueryArg => f.write_str("query or node is nil"),
            MatchError::NoMatch => f.write_str("no match found"),
            MatchError::Compilation(e) => {
                write!(f, "tree-sitter query compilation failed: {e}")
            }
        }
    }
}

impl std::error::Error for MatchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            MatchError::Compilation(e) => Some(e),
            _ => None,
        }
    }
}

/// Compiles and matches S-expression patterns to tree-sitter queries, with a
/// pattern→query cache and a cursor free list. Mirrors Go `PatternMatcher`.
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
    /// Creates a new matcher for `lang` with an empty cache. Mirrors Go
    /// `NewPatternMatcher`.
    pub fn new(lang: Language) -> Self {
        PatternMatcher {
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
    /// query. On a cache hit the hit counter is bumped; on a miss the pattern is
    /// compiled, stored, and the miss counter is bumped. Mirrors Go
    /// `CompileAndCache`.
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
        // value to match the Go single-writer behavior of the last writer
        // winning is not observable since queries are equivalent.
        let entry = inner
            .cache
            .entry(pattern.to_string())
            .or_insert_with(|| Arc::clone(&compiled));
        let result = Arc::clone(entry);
        inner.misses += 1;
        Ok(result)
    }

    /// Returns the number of cache hits and misses. Mirrors Go `CacheStats`.
    pub fn cache_stats(&self) -> (i64, i64) {
        let inner = self.inner.lock().expect("pattern matcher cache poisoned");
        (inner.hits, inner.misses)
    }

    /// Matches a compiled query against a node and returns the captures of the
    /// first match as a name→text map. Mirrors Go `MatchPattern`.
    ///
    /// The returned map is a [`BTreeMap`] for deterministic ordering. Go returns
    /// a `map[string]string` whose iteration order is randomized; the key/value
    /// contents are identical. Captures whose node is null are skipped, matching
    /// the Go `!cap.Node.IsNull()` guard.
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
            .unwrap_or_else(QueryCursor::new);

        let result = match_tree_sitter_query(query, &mut cursor, node, source);

        // Return the cursor to the pool (RAII analogue of sync.Pool.Put).
        self.cursor_pool
            .lock()
            .expect("cursor pool poisoned")
            .push(cursor);

        result
    }
}

/// Port of Go `matchTreeSitterQuery`: takes the first match's captures.
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
        // tree-sitter 0.22 query-match capture nodes are always present (there is
        // no null/`IsNull` notion as in go-tree-sitter-bare), so the Go
        // `!cap.Node.IsNull()` guard collapses to extracting the captured text.
        // `utf8_text` errors only on invalid UTF-8, which the Go `Content` path
        // would never receive, so skipping on error preserves behavior.
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
    // there. Here we only assert the error-display wording matches the Go
    // sentinels, which is part of the observable surface.
    use super::*;

    #[test]
    fn error_messages_match_go_sentinels() {
        assert_eq!(MatchError::NilQueryArg.to_string(), "query or node is nil");
        assert_eq!(MatchError::NoMatch.to_string(), "no match found");
    }
}

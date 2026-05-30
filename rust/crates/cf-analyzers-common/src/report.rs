//! Report value model and `safeconv`-compatible numeric coercions.
//!
//! In the Go codebase a report is `analyze.Report = map[string]any`
//! (`internal/analyzers/analyze/analyzer.go:26`). The values stored in a report
//! are a small, well-known set of dynamic types: numbers (which preserve their
//! Go static type — `int`, `int64`, `float64`, the `uint` family, …), strings,
//! booleans, collections of report items (`[]map[string]any`), and the deferred
//! [`TypedCollection`] wrapper used by the detailed/spillable collectors.
//!
//! The upstream `cf-analyze` crate (which owns `analyze.Report`,
//! `analyze.TypedCollection`, `analyze.AggregationMode`, and the `_source_file`
//! / `_language` / `_directory` key constants) is still a bare scaffold in this
//! workspace, so this module defines the *minimal* equivalents that
//! `cf-analyzers-common` depends on. The intended end state is for these types
//! to be re-exported from `cf-analyze` once that crate is implemented; the
//! follow-up is tracked in the crate-level roadmap note in `lib.rs`.
//!
//! Numeric coercion mirrors `pkg/safeconv/convert.go` exactly: `ToFloat64`
//! accepts the float and signed/unsigned integer families; `ToInt` additionally
//! truncates floats toward zero. Strings, bools, and containers are never
//! numeric.

use std::collections::BTreeMap;

/// Report key for the source file path (`analyze.SourceFileKey`, `_source_file`).
pub const SOURCE_FILE_KEY: &str = "_source_file";

/// Report key for the directory path (`analyze.DirectoryKey`, `_directory`).
pub const DIRECTORY_KEY: &str = "_directory";

/// Report key for the programming language (`analyze.LanguageKey`, `_language`).
pub const LANGUAGE_KEY: &str = "_language";

/// A single report item: the Rust analogue of Go's `map[string]any`.
///
/// Keys are stored in a [`BTreeMap`] so that iteration order is deterministic
/// (byte-sorted by key), matching how Go's `encoding/json` serializes maps. The
/// Go code itself iterates report items in nondeterministic map order, but the
/// only observable orderings (collection element order, JSON output) are either
/// independently sorted or sorted at encode time, so a sorted map here is a
/// faithful and strictly-more-deterministic stand-in.
pub type Item = BTreeMap<String, Value>;

/// A report: the Rust analogue of `analyze.Report` (`map[string]any`).
pub type Report = BTreeMap<String, Value>;

/// The dynamic value type stored in report items and reports.
///
/// This is a deliberately small enum covering exactly the `any` cases that the
/// `analyzers-common` package observes. The numeric variants preserve the Go
/// static type so that [`Value::to_float64`] / [`Value::to_int`] reproduce the
/// `safeconv` type switch and so that re-serialization can route ints and
/// floats down the correct `cf-gojson` paths.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Go `nil`.
    Null,
    /// Go `bool`.
    Bool(bool),
    /// Go `int` / `int32` / `int64` (and the `uint` family after coercion).
    Int(i64),
    /// Go `uint` / `uint32` / `uint64` kept distinct so unsigned semantics and
    /// JSON rendering match.
    Uint(u64),
    /// Go `float64` / `float32`.
    Float(f64),
    /// Go `string`.
    Str(String),
    /// A nested report item (`map[string]any`).
    Item(Item),
    /// A collection of report items (`[]map[string]any`).
    Collection(Vec<Item>),
    /// A list of arbitrary values (`[]any`), e.g. role slices.
    List(Vec<Value>),
    /// The deferred [`TypedCollection`] wrapper.
    Typed(TypedCollection),
}

impl Value {
    /// Reproduces `safeconv.ToFloat64`: accepts the float and signed/unsigned
    /// integer families, rejects everything else.
    ///
    /// Returns `Some(f)` on success, `None` for non-numeric values — exactly the
    /// `(float64, bool)` contract of the Go function.
    #[must_use]
    pub fn to_float64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            Value::Uint(u) => Some(*u as f64),
            _ => None,
        }
    }

    /// Reproduces `safeconv.ToInt`: accepts the integer families and also
    /// truncates floats toward zero (Go's `int(float)` conversion).
    ///
    /// Returns `Some(i)` on success, `None` for non-numeric values.
    #[must_use]
    pub fn to_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            // Go `safeUintToInt`/`safeUintToInt64` clamp at the max int; on the
            // 64-bit platforms codefang targets this saturates at i64::MAX.
            Value::Uint(u) => Some(i64::try_from(*u).unwrap_or(i64::MAX)),
            // Go `int(float64)` truncates toward zero.
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Returns the inner string if this value is a [`Value::Str`], matching
    /// Go's `v, ok := x.(string)` comma-ok assertion.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the inner collection if this value is a [`Value::Collection`].
    #[must_use]
    pub fn as_collection(&self) -> Option<&[Item]> {
        match self {
            Value::Collection(c) => Some(c),
            _ => None,
        }
    }

    /// Returns the inner [`TypedCollection`] if this value is a [`Value::Typed`].
    #[must_use]
    pub fn as_typed(&self) -> Option<&TypedCollection> {
        match self {
            Value::Typed(tc) => Some(tc),
            _ => None,
        }
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Value::Int(v as i64)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}

/// Opaque holder for the typed item slice carried by a [`TypedCollection`].
///
/// In Go this is `any` holding e.g. `[]FunctionInfo`. Since the concrete item
/// types live in not-yet-ported analyzer crates, the common crate only needs
/// the already-converted map form and the passthrough case; this enum captures
/// both without depending on those types.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedItems {
    /// Items already in map form (`[]map[string]any`) — the passthrough case.
    Maps(Vec<Item>),
}

/// The deferred-conversion collection wrapper, mirroring
/// `analyze.TypedCollection`.
///
/// It pairs a typed item slice with a converter and per-collection metadata so
/// that boxing items into maps is deferred until aggregation time.
#[derive(Debug, Clone)]
pub struct TypedCollection {
    /// Items already in map form (the passthrough converter case). When this is
    /// present, [`TypedCollection::to_maps`] returns a clone with `source_file`
    /// stamped, reproducing `passthroughConverter`.
    pub items: TypedItems,
    /// Source file path for these items.
    pub source_file: String,
    /// Programming language (stamped onto items in the detailed collector).
    pub language: String,
    /// Directory path (stamped onto items in the detailed collector).
    pub directory: String,
}

impl PartialEq for TypedCollection {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
            && self.source_file == other.source_file
            && self.language == other.language
            && self.directory == other.directory
    }
}

impl TypedCollection {
    /// Creates a [`TypedCollection`] from items already in map form and a source
    /// file, mirroring `analyze.NewTypedCollection` (the passthrough converter).
    #[must_use]
    pub fn new(items: Vec<Item>, source_file: impl Into<String>) -> Self {
        TypedCollection {
            items: TypedItems::Maps(items),
            source_file: source_file.into(),
            language: String::new(),
            directory: String::new(),
        }
    }

    /// Converts the wrapped items to report-item maps, stamping the source file
    /// onto any item that lacks a `_source_file` key.
    ///
    /// Reproduces `passthroughConverter.ToMaps`: maps are returned in order, and
    /// the source-file key is only added when absent.
    #[must_use]
    pub fn to_maps(&self) -> Vec<Item> {
        match &self.items {
            TypedItems::Maps(maps) => {
                let mut out = maps.clone();
                if !self.source_file.is_empty() {
                    for m in &mut out {
                        m.entry(SOURCE_FILE_KEY.to_string())
                            .or_insert_with(|| Value::Str(self.source_file.clone()));
                    }
                }
                out
            }
        }
    }

    /// Returns the number of wrapped items, mirroring `typedCollectionLen`.
    #[must_use]
    pub fn len(&self) -> usize {
        match &self.items {
            TypedItems::Maps(maps) => maps.len(),
        }
    }

    /// Returns `true` when the collection wraps no items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Aggregation mode, mirroring `analyze.AggregationMode`.
///
/// In [`AggregationMode::SummaryOnly`] the collectors disable per-item data
/// collection; [`AggregationMode::Full`] (the default) collects everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AggregationMode {
    /// Collect all per-item data (`AggregationModeFull`, the Go zero value).
    #[default]
    Full,
    /// Collect summary metrics only, skipping per-item data
    /// (`AggregationModeSummaryOnly`).
    SummaryOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_float64_accepts_numeric_families() {
        assert_eq!(Value::Float(0.8).to_float64(), Some(0.8));
        assert_eq!(Value::Int(5).to_float64(), Some(5.0));
        assert_eq!(Value::Uint(7).to_float64(), Some(7.0));
    }

    #[test]
    fn to_float64_rejects_non_numeric() {
        assert_eq!(Value::Str("x".into()).to_float64(), None);
        assert_eq!(Value::Bool(true).to_float64(), None);
        assert_eq!(Value::Null.to_float64(), None);
        assert_eq!(Value::Collection(vec![]).to_float64(), None);
    }

    #[test]
    fn to_int_truncates_floats() {
        assert_eq!(Value::Float(3.9).to_int(), Some(3));
        assert_eq!(Value::Float(-3.9).to_int(), Some(-3));
        assert_eq!(Value::Int(5).to_int(), Some(5));
        assert_eq!(Value::Str("x".into()).to_int(), None);
    }

    #[test]
    fn typed_collection_stamps_source_file() {
        let mut item = Item::new();
        item.insert("name".into(), Value::Str("a".into()));
        let tc = TypedCollection::new(vec![item], "src.go");
        let maps = tc.to_maps();
        assert_eq!(maps.len(), 1);
        assert_eq!(maps[0].get(SOURCE_FILE_KEY), Some(&Value::Str("src.go".into())));
    }

    #[test]
    fn typed_collection_keeps_existing_source_file() {
        let mut item = Item::new();
        item.insert(SOURCE_FILE_KEY.into(), Value::Str("orig.go".into()));
        let tc = TypedCollection::new(vec![item], "src.go");
        let maps = tc.to_maps();
        assert_eq!(maps[0].get(SOURCE_FILE_KEY), Some(&Value::Str("orig.go".into())));
    }
}

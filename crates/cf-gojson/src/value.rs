//! The dynamic report value model: [`GoValue`], [`GoMap`], and [`MapOrigin`].
//!
//! A [`GoValue`] covers every shape a report can carry — `null`, `bool`,
//! integers, floats, strings, arrays, and objects. It is the single value
//! model the whole report layer builds and that the [`crate::marshal`] encoder
//! consumes.
//!
//! # The dual-mode container is load-bearing
//!
//! The report format has two object-ordering rules, decided by the *origin* of
//! the JSON object (report-format contract; pinned by `tests/compat`):
//!
//! * a **struct-origin** object serializes its fields in **declaration
//!   order** (each field pushed once, in order; empty fields may be skipped by
//!   the builder — the `omitempty` convention); and
//! * a **map-origin** object serializes its keys **byte-sorted** at encode
//!   time.
//!
//! [`GoMap`] carries a [`MapOrigin`] tag so the encoder applies the right rule:
//! [`MapOrigin::Struct`] keeps insertion order, [`MapOrigin::Map`] byte-sorts on
//! encode. Internally a `GoMap` *always* stores entries in insertion order; the
//! sort happens only in [`GoMap::encode_order`] (and thus only at marshal time),
//! so accessors that iterate see a stable, predictable order.

use std::cmp::Ordering;

/// Whether a [`GoMap`] is a struct-origin or map-origin object.
///
/// This decides the key ordering the encoder applies (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapOrigin {
    /// A struct-origin object: fields keep source **declaration order**; the
    /// encoder does **not** sort them. The `omitempty` convention is the
    /// caller's responsibility (skip the `push`/`insert` when the value is
    /// empty).
    Struct,
    /// A map-origin object: keys are **byte-sorted** by the encoder at marshal
    /// time (report-format contract).
    Map,
    /// A map-origin object with **integer keys**, stored here as their
    /// decimal-string forms. JSON always quotes keys and sorts the *strings*
    /// lexicographically — identical to [`MapOrigin::Map`] — while the YAML
    /// emitter sorts these keys **numerically** and writes them as **plain
    /// `!!int` scalars** (unquoted). Both are reference-implementation
    /// behavior, pinned by the differential gate.
    IntMap,
}

/// A dynamic report value, covering every shape the report format marshals.
///
/// Integers are split into [`GoValue::Int`] (`i64`) and [`GoValue::Uint`]
/// (`u64`) so they never pass through the float formatter — integers encode as
/// plain decimal, never the float path. [`GoValue::Float`] is rendered by
/// [`crate::ftoa`] (contract float layout).
#[derive(Debug, Clone, PartialEq)]
pub enum GoValue {
    /// JSON `null`.
    Null,
    /// A **nil slice** — an absent collection, as opposed to a present-but-
    /// empty one. The report contract marshals this asymmetrically by encoder:
    /// JSON writes `null`, YAML writes `[]` (reference-implementation
    /// behavior). A distinct variant is required because neither
    /// [`GoValue::Null`] (which YAML renders `null`) nor
    /// [`GoValue::Array`]`(vec![])` (which JSON renders `[]`) reproduces both.
    /// An *initialized-but-empty* collection renders `[]` in both encoders and
    /// stays [`GoValue::Array`]`(vec![])`.
    NilSlice,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// An unsigned integer.
    Uint(u64),
    /// A 64-bit float.
    Float(f64),
    /// A UTF-8 string.
    Str(String),
    /// A JSON array.
    Array(Vec<GoValue>),
    /// A JSON object (struct- or map-origin); see [`GoMap`] / [`MapOrigin`].
    Map(GoMap),
}

impl GoValue {
    /// Builds an object [`GoValue`] from a [`GoMap`].
    ///
    /// This is an alias for the [`GoValue::Map`] variant, provided so call sites
    /// that think in terms of "object" (struct- or map-origin) read naturally:
    /// `GoValue::object(m)` is identical to `GoValue::Map(m)`. Kept as an
    /// associated function (capitalized to mirror the variant) so existing
    /// `GoValue::Object(m)` construction sites compile unchanged.
    #[must_use]
    pub fn object(m: GoMap) -> GoValue {
        GoValue::Map(m)
    }

    /// Alias constructor matching the historical `GoValue::Object(..)` spelling.
    ///
    /// Identical to [`GoValue::object`]; both produce [`GoValue::Map`].
    #[allow(non_snake_case)]
    #[must_use]
    pub fn Object(m: GoMap) -> GoValue {
        GoValue::Map(m)
    }

    /// Returns the contained [`GoMap`] if this is an object value.
    #[must_use]
    pub fn as_map(&self) -> Option<&GoMap> {
        match self {
            GoValue::Map(m) => Some(m),
            _ => None,
        }
    }

    /// Returns the contained string if this is a [`GoValue::Str`].
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            GoValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Returns `true` for the empty/zero JSON shapes the `omitempty` convention
    /// drops: `null`, `false`, `0`, `0.0`, `""`, `[]`, and the empty map.
    ///
    /// Provided as a convenience for callers building struct-origin maps; the
    /// encoder itself never consults it (struct field skipping is decided when
    /// the field is `push`ed).
    #[must_use]
    pub fn is_go_empty(&self) -> bool {
        match self {
            GoValue::Null | GoValue::NilSlice => true,
            GoValue::Bool(b) => !*b,
            GoValue::Int(i) => *i == 0,
            GoValue::Uint(u) => *u == 0,
            GoValue::Float(f) => *f == 0.0,
            GoValue::Str(s) => s.is_empty(),
            GoValue::Array(a) => a.is_empty(),
            GoValue::Map(m) => m.is_empty(),
        }
    }
}

/// An insertion-ordered string-keyed object that marshals with the ordering rule
/// chosen by its [`MapOrigin`].
///
/// * Storage is always insertion-ordered (`Vec<(String, GoValue)>`), so
///   iteration via [`GoMap::iter`] / [`GoMap::entries`] is stable.
/// * [`GoMap::encode_order`] returns the entries in the order the encoder will
///   write them: insertion order for [`MapOrigin::Struct`], byte-sorted keys for
///   [`MapOrigin::Map`] (report-format contract).
///
/// Re-inserting an existing key **updates in place** (keeping its original
/// position), so a struct field is never duplicated and a map key is unique —
/// plain last-wins map semantics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoMap {
    origin: MapOrigin,
    entries: Vec<(String, GoValue)>,
}

impl Default for MapOrigin {
    /// Defaults to [`MapOrigin::Map`] so a `GoMap::default()` byte-sorts keys —
    /// the safe choice for the dominant report-object shape.
    fn default() -> Self {
        MapOrigin::Map
    }
}

impl GoMap {
    /// Creates an empty map with the given [`MapOrigin`].
    #[must_use]
    pub fn new(origin: MapOrigin) -> Self {
        GoMap {
            origin,
            entries: Vec::new(),
        }
    }

    /// Creates an empty **map-origin** object (keys byte-sorted on encode).
    #[must_use]
    pub fn new_map() -> Self {
        GoMap::new(MapOrigin::Map)
    }

    /// Creates an empty **int-map-origin** object: decimal-string keys that
    /// originate from an integer type (see [`MapOrigin::IntMap`]; YAML sorts
    /// them numerically and emits plain `!!int` scalars).
    ///
    /// Insert keys via `i.to_string()`.
    #[must_use]
    pub fn new_int_map() -> Self {
        GoMap::new(MapOrigin::IntMap)
    }

    /// Creates an empty **struct-origin** object (fields keep declaration order).
    ///
    /// `push` fields in declaration order.
    #[must_use]
    pub fn new_struct() -> Self {
        GoMap::new(MapOrigin::Struct)
    }

    /// Builds a **map-origin** object from `(key, value)` pairs.
    ///
    /// Keys byte-sort at encode time (the [`MapOrigin::Map`] rule), and a
    /// repeated key keeps its **last** value (last-wins). Storage stays
    /// insertion-ordered; the sort happens only in [`GoMap::encode_order`].
    #[must_use]
    pub fn from_map(entries: Vec<(String, GoValue)>) -> Self {
        let mut m = GoMap::new(MapOrigin::Map);
        for (k, v) in entries {
            m.insert(k, v); // insert = last-wins, position-stable
        }
        m
    }

    /// Returns this object's [`MapOrigin`].
    #[must_use]
    pub fn origin(&self) -> MapOrigin {
        self.origin
    }

    /// Inserts or updates `key`.
    ///
    /// If `key` is already present its value is replaced **in place** (the entry
    /// keeps its original position). Otherwise the entry is appended
    /// (insertion order).
    pub fn insert(&mut self, key: impl Into<String>, value: GoValue) {
        let key = key.into();
        if let Some(slot) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    /// Appends `key`/`value` without checking for duplicates.
    ///
    /// This is the fast path for **struct-origin** building, where each field is
    /// pushed exactly once in declaration order, so a uniqueness scan is
    /// unnecessary. (For maps with possibly-repeated keys use [`GoMap::insert`].)
    pub fn push(&mut self, key: impl Into<String>, value: GoValue) {
        self.entries.push((key.into(), value));
    }

    /// Returns a reference to the value for `key`, if present.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&GoValue> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Returns `true` if `key` is present.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// Iterates entries in **insertion order** as `(&String, &GoValue)`.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &GoValue)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }

    /// Iterates keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().map(|(k, _)| k)
    }

    /// Iterates values in insertion order.
    pub fn values(&self) -> impl Iterator<Item = &GoValue> {
        self.entries.iter().map(|(_, v)| v)
    }

    /// Returns the backing entries in insertion order.
    #[must_use]
    pub fn entries(&self) -> &[(String, GoValue)] {
        &self.entries
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the object is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the entries in the exact order the encoder will write them.
    ///
    /// * [`MapOrigin::Struct`] → insertion order (a borrow of the storage).
    /// * [`MapOrigin::Map`] → keys byte-sorted (`key.as_bytes()` lexicographic;
    ///   report-format contract).
    ///
    /// The returned vector borrows the entries, so no values are cloned.
    #[must_use]
    pub fn encode_order(&self) -> Vec<&(String, GoValue)> {
        let mut refs: Vec<&(String, GoValue)> = self.entries.iter().collect();
        match self.origin {
            // Map keys sort by raw byte order. This also covers `IntMap`: the
            // JSON contract stringifies integer keys and sorts the STRINGS
            // lexically — `{"10":…,"2":…}` — so for JSON an int-keyed map
            // orders exactly like a string-keyed one. (Only the YAML encoder
            // treats `IntMap` differently: numeric sort + unquoted keys.)
            MapOrigin::Map | MapOrigin::IntMap => refs.sort_by(|a, b| cmp_keys(&a.0, &b.0)),
            MapOrigin::Struct => {}
        }
        refs
    }
}

/// Compares two object keys by raw UTF-8 byte sequence (bytewise
/// lexicographic — the report-format key order).
fn cmp_keys(a: &str, b: &str) -> Ordering {
    a.as_bytes().cmp(b.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_origin_default_is_map() {
        assert_eq!(MapOrigin::default(), MapOrigin::Map);
        assert_eq!(GoMap::default().origin(), MapOrigin::Map);
    }

    #[test]
    fn insert_then_get_round_trip() {
        let mut m = GoMap::new_map();
        m.insert("a", GoValue::Int(1));
        m.insert("b", GoValue::Str("x".into()));
        assert_eq!(m.get("a"), Some(&GoValue::Int(1)));
        assert_eq!(m.get("b"), Some(&GoValue::Str("x".into())));
        assert_eq!(m.get("missing"), None);
        assert!(m.contains_key("a"));
        assert!(!m.contains_key("z"));
        assert_eq!(m.len(), 2);
        assert!(!m.is_empty());
    }

    #[test]
    fn insert_updates_in_place_keeping_position() {
        let mut m = GoMap::new_struct();
        m.insert("first", GoValue::Int(1));
        m.insert("second", GoValue::Int(2));
        m.insert("first", GoValue::Int(99)); // update, not append
        let order: Vec<&str> = m.keys().map(String::as_str).collect();
        assert_eq!(order, ["first", "second"]);
        assert_eq!(m.get("first"), Some(&GoValue::Int(99)));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn struct_origin_keeps_declaration_order() {
        let mut m = GoMap::new_struct();
        m.push("zebra", GoValue::Int(1));
        m.push("apple", GoValue::Int(2));
        m.push("mango", GoValue::Int(3));
        let order: Vec<&str> = m.encode_order().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(order, ["zebra", "apple", "mango"]);
    }

    #[test]
    fn map_origin_byte_sorts_keys_on_encode() {
        let mut m = GoMap::new_map();
        // Insert in scrambled order; byte order is A(0x41) < Z(0x5a) < [(0x5b)
        // < a(0x61) < z(0x7a).
        for k in ["z", "a", "Z", "A", "["] {
            m.push(k, GoValue::Null);
        }
        let order: Vec<&str> = m.encode_order().iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(order, ["A", "Z", "[", "a", "z"]);
        // Storage iteration stays insertion-ordered.
        let stored: Vec<&str> = m.keys().map(String::as_str).collect();
        assert_eq!(stored, ["z", "a", "Z", "A", "["]);
    }

    #[test]
    fn object_alias_constructors_match_variant() {
        let mut m = GoMap::new_map();
        m.insert("k", GoValue::Int(1));
        assert_eq!(GoValue::Object(m.clone()), GoValue::Map(m.clone()));
        assert_eq!(GoValue::object(m.clone()), GoValue::Map(m));
    }

    #[test]
    fn nil_slice_is_go_empty() {
        // A nil slice is an `omitempty` zero (dropped like any empty slice).
        assert!(GoValue::NilSlice.is_go_empty());
    }

    #[test]
    fn is_go_empty_matches_omitempty_zero_values() {
        assert!(GoValue::Null.is_go_empty());
        assert!(GoValue::Bool(false).is_go_empty());
        assert!(!GoValue::Bool(true).is_go_empty());
        assert!(GoValue::Int(0).is_go_empty());
        assert!(!GoValue::Int(1).is_go_empty());
        assert!(GoValue::Uint(0).is_go_empty());
        assert!(GoValue::Float(0.0).is_go_empty());
        assert!(GoValue::Str(String::new()).is_go_empty());
        assert!(GoValue::Array(vec![]).is_go_empty());
        assert!(GoValue::Map(GoMap::new_map()).is_go_empty());
        assert!(!GoValue::Str("x".into()).is_go_empty());
    }
}

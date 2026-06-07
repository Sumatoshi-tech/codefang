//! The dynamic Go value model: [`GoValue`], [`GoMap`], and [`MapOrigin`].
//!
//! This is the Rust analogue of the Go values that `encoding/json.Marshal`
//! serializes from `any` — `null`, `bool`, integers, `float64`, `string`,
//! slices, and maps/structs. It is the single value model the whole report
//! layer builds and that the [`crate::marshal`] encoder consumes.
//!
//! # The dual-mode container is load-bearing
//!
//! Go's `encoding/json` has two ordering rules that depend on the *origin* of a
//! JSON object:
//!
//! * a Go **struct** serializes its fields in source **declaration order**
//!   (honoring `json:"name,omitempty"`); and
//! * a Go **map** (`map[string]any`, `map[string]int`, …) serializes its keys
//!   **byte-sorted** by the runtime at encode time.
//!
//! [`GoMap`] carries a [`MapOrigin`] tag so the encoder applies the right rule:
//! [`MapOrigin::Struct`] keeps insertion order, [`MapOrigin::Map`] byte-sorts on
//! encode. Internally a `GoMap` *always* stores entries in insertion order; the
//! sort happens only in [`GoMap::encode_order`] (and thus only at marshal time),
//! so accessors that iterate see a stable, predictable order.

use std::cmp::Ordering;

/// Whether a [`GoMap`] originated from a Go struct or a Go map.
///
/// This decides the key ordering the encoder applies (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapOrigin {
    /// A Go struct: fields keep source **declaration order**; the encoder does
    /// **not** sort them. `omitempty` is the caller's responsibility (skip the
    /// `push`/`insert` when the value is empty).
    Struct,
    /// A Go map: keys are **byte-sorted** by the encoder at marshal time, exactly
    /// as the Go runtime sorts `map[string]…` keys when encoding JSON.
    Map,
    /// A Go map with **integer keys** (`map[int]…`), whose decimal-string keys
    /// are stored here but originate from an integer type. The encoders sort
    /// these keys **numerically** (Go orders integer map keys by value, not
    /// lexicographically), and the YAML emitter writes them as **plain `!!int`
    /// scalars** (unquoted), matching `gopkg.in/yaml.v3`'s `map[int]…` output.
    /// JSON keys are always quoted strings regardless, so JSON output differs
    /// from [`MapOrigin::Map`] only in the numeric key ordering.
    IntMap,
}

/// A dynamic Go value, covering every shape `encoding/json` marshals from `any`.
///
/// Integers are split into [`GoValue::Int`] (`i64`, covering Go `int`/`int64`/…)
/// and [`GoValue::Uint`] (`u64`, covering Go `uint`/`uint64`) so they never pass
/// through the float formatter — Go encodes integers with `strconv.AppendInt`,
/// not the float path. [`GoValue::Float`] is rendered by [`crate::ftoa`] to match
/// Go's `encoding/json` float encoder byte-for-byte.
#[derive(Debug, Clone, PartialEq)]
pub enum GoValue {
    /// JSON `null` (Go `nil`).
    Null,
    /// Go `bool`.
    Bool(bool),
    /// A signed integer (Go `int`, `int8`..`int64`).
    Int(i64),
    /// An unsigned integer (Go `uint`, `uint8`..`uint64`).
    Uint(u64),
    /// A 64-bit float (Go `float32` promoted to `f64`, or `float64`).
    Float(f64),
    /// A UTF-8 string (Go `string`).
    Str(String),
    /// A JSON array (Go slice/array).
    Array(Vec<GoValue>),
    /// A JSON object (Go struct or map); see [`GoMap`] / [`MapOrigin`].
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

    /// Returns `true` for the empty/zero JSON shapes that Go's `omitempty`
    /// would drop: `null`, `false`, `0`, `0.0`, `""`, `[]`, and the empty map.
    ///
    /// Provided as a convenience for callers building struct-origin maps; the
    /// encoder itself never consults it (struct field skipping is decided when
    /// the field is `push`ed, exactly as Go decides it at compile time).
    #[must_use]
    pub fn is_go_empty(&self) -> bool {
        match self {
            GoValue::Null => true,
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
///   [`MapOrigin::Map`] (Go runtime behavior).
///
/// Insert semantics mirror Go: re-inserting an existing key **updates in place**
/// (keeping its original position), so a struct field is never duplicated and a
/// map key is unique — matching `map[k] = v`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GoMap {
    origin: MapOrigin,
    entries: Vec<(String, GoValue)>,
}

impl Default for MapOrigin {
    /// Defaults to [`MapOrigin::Map`] so a `GoMap::default()` byte-sorts keys —
    /// the safe choice for the dominant `map[string]any` report shape.
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
    ///
    /// Use this for Go `map[string]…` values.
    #[must_use]
    pub fn new_map() -> Self {
        GoMap::new(MapOrigin::Map)
    }

    /// Creates an empty **int-map-origin** object (decimal-string keys that
    /// originate from an integer type; sorted numerically on encode, emitted as
    /// plain `!!int` YAML scalars).
    ///
    /// Use this for Go `map[int]…` values. Insert keys via `i.to_string()`.
    #[must_use]
    pub fn new_int_map() -> Self {
        GoMap::new(MapOrigin::IntMap)
    }

    /// Creates an empty **struct-origin** object (fields keep declaration order).
    ///
    /// Use this for Go structs; `push` fields in source declaration order.
    #[must_use]
    pub fn new_struct() -> Self {
        GoMap::new(MapOrigin::Struct)
    }

    /// Builds a **map-origin** object from `(key, value)` pairs.
    ///
    /// Mirrors constructing a Go `map[string]any` from entries: keys byte-sort at
    /// encode time (the [`MapOrigin::Map`] rule), and a repeated key keeps its
    /// **last** value (last-wins), exactly like successive `m[k] = v` assignments.
    /// Storage stays insertion-ordered; the sort happens only in
    /// [`GoMap::encode_order`].
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
    /// keeps its original position), mirroring Go's `m[key] = value`. Otherwise
    /// the entry is appended (insertion order).
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
    /// * [`MapOrigin::Map`] → keys byte-sorted (`key.as_bytes()` lexicographic),
    ///   exactly how Go's runtime orders `map[string]…` keys for JSON.
    ///
    /// The returned vector borrows the entries, so no values are cloned.
    #[must_use]
    pub fn encode_order(&self) -> Vec<&(String, GoValue)> {
        let mut refs: Vec<&(String, GoValue)> = self.entries.iter().collect();
        match self.origin {
            // Go sorts map keys by raw byte order. `str`'s `Ord` is byte-wise
            // (it compares `as_bytes()`), so this matches Go exactly. This also
            // covers `IntMap`: `encoding/json` stringifies integer keys and then
            // sorts the STRINGS lexically — `{"10":…,"2":…}` — so for JSON an
            // int-keyed map orders exactly like a string-keyed one. (Only the
            // YAML encoder treats `IntMap` differently: numeric sort + unquoted
            // keys, matching yaml.v3.)
            MapOrigin::Map | MapOrigin::IntMap => refs.sort_by(|a, b| cmp_keys(&a.0, &b.0)),
            MapOrigin::Struct => {}
        }
        refs
    }
}

/// Compares two object keys the way Go orders map keys for JSON: by raw UTF-8
/// byte sequence (`bytes.Compare` semantics).
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

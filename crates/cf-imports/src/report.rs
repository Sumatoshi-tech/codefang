//! Dynamic report value model.
//!
//! The history and plot paths hand the analyzer a string-keyed value tree
//! (the `imports` list, or the nested per-developer map); [`ReportValue`] is
//! that tree. Machine output never serializes it directly — every machine
//! format routes through [`crate::ComputedMetrics::to_go_value`] and the
//! `cf-gojson` / `cf-goyaml` encoders.

use std::collections::BTreeMap;

/// A value in a report tree (dynamic, string-keyed).
///
/// Map keys use a [`BTreeMap`] so iteration order is byte-sorted.
#[derive(Debug, Clone, PartialEq)]
pub enum ReportValue {
    /// JSON `null`.
    Null,
    /// A boolean.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// A 64-bit float.
    Float(f64),
    /// A UTF-8 string.
    Str(String),
    /// An ordered list.
    List(Vec<ReportValue>),
    /// A string-keyed map with byte-sorted iteration order.
    Map(BTreeMap<String, ReportValue>),
}

impl ReportValue {
    /// Convenience constructor for an empty map.
    #[must_use]
    pub fn map() -> Self {
        ReportValue::Map(BTreeMap::new())
    }

    /// Inserts a key/value into a [`ReportValue::Map`].
    ///
    /// # Panics
    /// Panics if `self` is not a [`ReportValue::Map`].
    pub fn insert(&mut self, key: impl Into<String>, value: ReportValue) {
        match self {
            ReportValue::Map(m) => {
                m.insert(key.into(), value);
            }
            _ => panic!("ReportValue::insert called on non-map value"),
        }
    }

    /// Borrows the inner map, if this is a [`ReportValue::Map`].
    #[must_use]
    pub fn as_map(&self) -> Option<&BTreeMap<String, ReportValue>> {
        match self {
            ReportValue::Map(m) => Some(m),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_insert_and_lookup() {
        let mut m = ReportValue::map();
        m.insert("b", ReportValue::Int(2));
        m.insert("a", ReportValue::Str("x".into()));
        let keys: Vec<&String> = m.as_map().expect("map").keys().collect();
        assert_eq!(keys, ["a", "b"]);
        assert!(ReportValue::Null.as_map().is_none());
    }

    #[test]
    #[should_panic(expected = "non-map")]
    fn insert_on_non_map_panics() {
        ReportValue::Int(1).insert("k", ReportValue::Null);
    }
}

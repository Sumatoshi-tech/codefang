//! The dynamic value model for reports: the Rust analogue of Go's
//! `Report = map[string]any` and the values its accessors switch on.
//!
//! Go stores heterogeneous values in `map[string]any` and recovers them with
//! type assertions. Rust has no runtime type assertions, so [`ReportValue`] is a
//! closed enum carrying exactly the concrete cases the Go `reportutil`
//! accessors recognize. Numeric variants preserve Go's `int`/`int64`/`float64`
//! distinction so [`crate::get_int`] / [`crate::get_float64`] coerce exactly as
//! `safeconv.ToInt` / `safeconv.ToFloat64` do.

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use cf_gojson::{GoMap, GoValue};
use cf_safeconv::GoNumber;

/// A report: the Rust analogue of Go's `Report = map[string]any`.
///
/// Backed by a `HashMap`; iteration order is unspecified here, but CFB1 / JSON
/// encoding routes through [`cf_gojson`] which byte-sorts map keys at encode
/// time (matching Go's `map[string]any` JSON encoding), so output ordering is
/// deterministic regardless of insertion order.
pub type Report = HashMap<String, ReportValue>;

/// Implemented by collection types that can expose themselves as a slice of
/// report maps.
///
/// Mirrors the Go `mapSlicer` interface (`MapSlice() []map[string]any`),
/// satisfied by `analyze.TypedCollection` without importing the `analyze`
/// package. [`crate::get_functions`] uses it as the escape hatch for typed
/// collections.
pub trait MapSlicer: Debug + Send + Sync {
    /// Returns the collection's elements as report maps. Mirrors Go `MapSlice`.
    fn map_slice(&self) -> Vec<Report>;
}

/// A dynamic value stored in a [`Report`].
///
/// Each variant corresponds to a Go dynamic type the `reportutil` accessors
/// handle. Unknown / unsupported Go values map to [`ReportValue::Other`], which
/// every typed accessor treats as "absent / wrong type" (returning the Go zero
/// value), and which encodes via its embedded [`GoValue`].
#[derive(Debug, Clone)]
pub enum ReportValue {
    /// JSON `null` / Go `nil`.
    Null,
    /// Go `bool`.
    Bool(bool),
    /// Go `int` / `int64` (modelled as `i64`).
    Int(i64),
    /// Go `int32`.
    Int32(i32),
    /// Go `uint` / `uint64`.
    Uint(u64),
    /// Go `float64`.
    Float(f64),
    /// Go `float32`.
    Float32(f32),
    /// Go `string`.
    Str(String),
    /// Go `[]string`.
    StringSlice(Vec<String>),
    /// Go `map[string]int`.
    StringIntMap(HashMap<String, i64>),
    /// Go `[]map[string]any` (the direct function-list case of `GetFunctions`).
    Functions(Vec<Report>),
    /// A nested report (`map[string]any`).
    Map(Report),
    /// A [`MapSlicer`] (e.g. `analyze.TypedCollection`) — the interface case of
    /// `GetFunctions`.
    MapSlice(Arc<dyn MapSlicer>),
    /// Any other value, carried as a pre-built [`GoValue`] for encoding. Typed
    /// accessors treat this as "wrong type".
    Other(GoValue),
}

impl PartialEq for ReportValue {
    fn eq(&self, other: &Self) -> bool {
        use ReportValue::*;
        match (self, other) {
            (Null, Null) => true,
            (Bool(a), Bool(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Int32(a), Int32(b)) => a == b,
            (Uint(a), Uint(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Float32(a), Float32(b)) => a == b,
            (Str(a), Str(b)) => a == b,
            (StringSlice(a), StringSlice(b)) => a == b,
            (StringIntMap(a), StringIntMap(b)) => a == b,
            (Functions(a), Functions(b)) => a == b,
            (Map(a), Map(b)) => a == b,
            (Other(a), Other(b)) => a == b,
            // MapSlice has no meaningful value equality (trait object).
            _ => false,
        }
    }
}

impl ReportValue {
    /// Returns the value as a [`GoNumber`] if it is one of Go's numeric types,
    /// for coercion via `safeconv`. Non-numeric values return `None`.
    ///
    /// Mirrors which dynamic types `safeconv.ToInt` / `safeconv.ToFloat64`
    /// accept (`int`, `int32`, `int64`, `uint`, `uint64`, `float32`, `float64`).
    pub fn as_go_number(&self) -> Option<GoNumber> {
        match *self {
            ReportValue::Int(v) => Some(GoNumber::Int(v)),
            ReportValue::Int32(v) => Some(GoNumber::Int32(v)),
            ReportValue::Uint(v) => Some(GoNumber::Uint64(v)),
            ReportValue::Float(v) => Some(GoNumber::Float64(v)),
            ReportValue::Float32(v) => Some(GoNumber::Float32(v)),
            _ => None,
        }
    }

    /// Returns the value as a string slice if it is a [`ReportValue::Str`].
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ReportValue::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Converts to the [`cf_gojson`] value model for CFB1 / JSON encoding.
    ///
    /// Nested maps and the function-list case become *map-origin* objects, so
    /// [`cf_gojson`] byte-sorts their keys at encode time exactly like Go's
    /// `map[string]any` encoding.
    pub fn to_govalue(&self) -> GoValue {
        match self {
            ReportValue::Null => GoValue::Null,
            ReportValue::Bool(b) => GoValue::Bool(*b),
            ReportValue::Int(i) => GoValue::Int(*i),
            ReportValue::Int32(i) => GoValue::Int(*i as i64),
            ReportValue::Uint(u) => GoValue::Uint(*u),
            ReportValue::Float(f) => GoValue::Float(*f),
            ReportValue::Float32(f) => GoValue::Float(*f as f64),
            ReportValue::Str(s) => GoValue::Str(s.clone()),
            ReportValue::StringSlice(items) => {
                GoValue::Array(items.iter().map(|s| GoValue::Str(s.clone())).collect())
            }
            ReportValue::StringIntMap(m) => GoValue::Object(map_string_int_to_gomap(m)),
            ReportValue::Functions(fns) => {
                GoValue::Array(fns.iter().map(report_to_govalue).collect())
            }
            ReportValue::Map(r) => report_to_govalue(r),
            ReportValue::MapSlice(slicer) => {
                GoValue::Array(slicer.map_slice().iter().map(report_to_govalue).collect())
            }
            ReportValue::Other(v) => v.clone(),
        }
    }
}

/// Converts a [`Report`] to a map-origin [`GoValue::Object`].
fn report_to_govalue(report: &Report) -> GoValue {
    let mut m = GoMap::new_map();
    for (k, v) in report {
        m.insert(k.clone(), v.to_govalue());
    }
    GoValue::Object(m)
}

/// Converts a `map[string]int` to a map-origin [`GoMap`] of integers.
fn map_string_int_to_gomap(m: &HashMap<String, i64>) -> GoMap {
    let mut out = GoMap::new_map();
    for (k, v) in m {
        out.insert(k.clone(), GoValue::Int(*v));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_go_number_covers_numeric_variants() {
        assert!(ReportValue::Int(1).as_go_number().is_some());
        assert!(ReportValue::Float(1.0).as_go_number().is_some());
        assert!(ReportValue::Uint(1).as_go_number().is_some());
        assert!(ReportValue::Str("x".into()).as_go_number().is_none());
        assert!(ReportValue::Null.as_go_number().is_none());
    }

    #[test]
    fn to_govalue_string_slice_is_array() {
        let v = ReportValue::StringSlice(vec!["a".into(), "b".into()]);
        match v.to_govalue() {
            GoValue::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn map_slice_trait_object_works() {
        #[derive(Debug)]
        struct Tc;
        impl MapSlicer for Tc {
            fn map_slice(&self) -> Vec<Report> {
                let mut r = Report::new();
                r.insert("name".into(), ReportValue::Str("foo".into()));
                vec![r]
            }
        }
        let v = ReportValue::MapSlice(Arc::new(Tc));
        match v.to_govalue() {
            GoValue::Array(items) => assert_eq!(items.len(), 1),
            other => panic!("expected array, got {other:?}"),
        }
    }
}

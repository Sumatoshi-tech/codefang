//! Type-safe accessors for report maps (`map[string]any` in Go).
//!
//! Port of the scalar-extraction half of
//! `internal/analyzers/common/reportutil/reportutil.go`. In Go these operate on
//! `map[string]any`; here the dynamic report is a [`cf_gojson::GoMap`] of
//! [`cf_gojson::GoValue`]s (the same model the serializer consumes), so the
//! accessors mirror Go's type-assertion and cross-type-coercion behavior over
//! that tree.
//!
//! `get_float64` / `get_int` delegate to [`cf_safeconv`] for the exact coercion
//! rules (reportutil.go:40,56). String/slice/map accessors mirror Go's direct
//! type assertion: a present-but-wrong-typed value yields the zero value.

use cf_gojson::{GoMap, GoValue};
use cf_safeconv::{Number, Value};

/// Bridges a [`GoValue`] to the dynamic [`cf_safeconv::Value`] used by the
/// numeric extractors.
///
/// Only the numeric / string / bool / null kinds participate, matching the
/// subset Go's `safeconv.Extract` observes. Containers (array, map) map to
/// [`Value::Nil`] so they are treated as non-numeric (Go's reflect-based switch
/// returns `ok == false` for them).
fn to_safeconv_value(v: &GoValue) -> Value {
    match v {
        GoValue::Int(i) => Value::Number(Number::Int64(*i)),
        GoValue::Uint(u) => Value::Number(Number::Uint64(*u)),
        GoValue::Float(f) => Value::Number(Number::Float64(*f)),
        GoValue::Str(s) => Value::String(s.clone()),
        GoValue::Bool(b) => Value::Bool(*b),
        GoValue::Null | GoValue::Array(_) | GoValue::Map(_) => Value::Nil,
    }
}

/// Looks up a key in a report map, returning the raw value if present.
///
/// This is the [`GoValue`]-typed analogue of Go's `report[key]`. Because Rust
/// has no runtime `any`-cast, the generic Go `GetAs[T]` is expressed through the
/// typed accessors below (`get_string`, `get_string_slice`, …) plus this raw
/// lookup. Key comparison is exact, matching Go map semantics.
#[must_use]
pub fn get<'a>(report: &'a GoMap, key: &str) -> Option<&'a GoValue> {
    report
        .entries()
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
}

/// Returns an `f64` from the report, coercing across numeric types.
///
/// Delegates to [`cf_safeconv::to_float64`] for consistent type handling. A
/// missing key or non-coercible value yields `0.0`. Mirrors `GetFloat64`
/// (reportutil.go:34).
#[must_use]
pub fn get_float64(report: &GoMap, key: &str) -> f64 {
    match get(report, key) {
        Some(v) => {
            let (f, valid) = cf_safeconv::to_float64(&to_safeconv_value(v));
            if valid {
                f
            } else {
                0.0
            }
        }
        None => 0.0,
    }
}

/// Returns an `i64` from the report, coercing across numeric types.
///
/// Delegates to [`cf_safeconv::to_int`] for consistent type handling. A missing
/// key or non-coercible value yields `0`. Mirrors `GetInt` (reportutil.go:50).
///
/// Note: Go's `int` is platform-width (64-bit on the supported targets). The
/// safeconv port returns `isize`; this accessor widens it to `i64` to give a
/// fixed return type on every target while matching the value on 64-bit hosts.
#[must_use]
pub fn get_int(report: &GoMap, key: &str) -> i64 {
    match get(report, key) {
        Some(v) => {
            let (i, valid) = cf_safeconv::to_int(&to_safeconv_value(v));
            if valid {
                i as i64
            } else {
                0
            }
        }
        None => 0,
    }
}

/// Returns a string from the report.
///
/// A missing key, or a present value that is not a string, yields `""`,
/// mirroring Go's `GetAs[string]` direct type assertion (reportutil.go:65).
#[must_use]
pub fn get_string(report: &GoMap, key: &str) -> String {
    match get(report, key) {
        Some(GoValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Returns a `Vec<String>` from the report.
///
/// Mirrors `GetStringSlice` / `GetAs[[]string]` (reportutil.go:96): a missing
/// key, or a value that is not an array of strings, yields an empty vector
/// (Go's `nil`). To match Go, a *mixed* array (one element not a string) is not
/// a `[]string` and yields empty.
#[must_use]
pub fn get_string_slice(report: &GoMap, key: &str) -> Vec<String> {
    match get(report, key) {
        Some(GoValue::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                if let GoValue::Str(s) = item {
                    out.push(s.clone());
                } else {
                    // Not a `[]string` in Go terms → assertion fails → nil.
                    return Vec::new();
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Returns the `[]map[string]any` for the given key as borrowed maps.
///
/// Mirrors `GetFunctions` (reportutil.go:78): handles a value that is an array
/// of objects. The Go `mapSlicer` (`TypedCollection`) fallback is represented by
/// any array of objects, since a parsed/serialized report exposes the flattened
/// `[]map[string]any` form. A missing key, or a value that is not an array of
/// objects, yields an empty vector (Go's `nil`).
#[must_use]
pub fn get_functions<'a>(report: &'a GoMap, key: &str) -> Vec<&'a GoMap> {
    match get(report, key) {
        Some(GoValue::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                if let GoValue::Map(m) = item {
                    out.push(m);
                } else {
                    return Vec::new();
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Returns a `map[string]int` for the given key as ordered `(key, i64)` pairs.
///
/// Mirrors `GetStringIntMap` / `GetAs[map[string]int]` (reportutil.go:103): the
/// value must be an object whose every value is an integer. A missing key, or
/// any non-integer member, yields an empty vector (Go's `nil`). Entries are
/// returned in encode order (byte-sorted for map-origin maps).
#[must_use]
pub fn get_string_int_map(report: &GoMap, key: &str) -> Vec<(String, i64)> {
    match get(report, key) {
        Some(GoValue::Map(m)) => {
            let entries = m.encode_order();
            let mut out = Vec::with_capacity(entries.len());
            for (k, v) in entries {
                match v {
                    GoValue::Int(i) => out.push((k.to_string(), *i)),
                    GoValue::Uint(u) => out.push((k.to_string(), *u as i64)),
                    _ => return Vec::new(),
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Returns a string from a map. Alias of [`get_string`].
///
/// Mirrors `MapString` (reportutil.go:110), which is `GetAs[string]` over an
/// arbitrary `map[string]any`.
#[must_use]
pub fn map_string(m: &GoMap, key: &str) -> String {
    get_string(m, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::MapOrigin;

    fn report(pairs: Vec<(&str, GoValue)>) -> GoMap {
        let mut m = GoMap::new(MapOrigin::Map);
        for (k, v) in pairs {
            m.insert(k, v);
        }
        m
    }

    // TestGetFloat64_Float (reportutil_test.go:7).
    #[test]
    fn get_float64_float() {
        let r = report(vec![("key", GoValue::Float(3.14))]);
        assert_eq!(get_float64(&r, "key"), 3.14);
    }

    // TestGetFloat64_Int (reportutil_test.go:16).
    #[test]
    fn get_float64_int() {
        let r = report(vec![("key", GoValue::Int(5))]);
        assert_eq!(get_float64(&r, "key"), 5.0);
    }

    // TestGetFloat64_Missing (reportutil_test.go:25).
    #[test]
    fn get_float64_missing() {
        let r = report(vec![]);
        assert_eq!(get_float64(&r, "key"), 0.0);
    }

    // TestGetInt_Int (reportutil_test.go:34).
    #[test]
    fn get_int_int() {
        let r = report(vec![("key", GoValue::Int(42))]);
        assert_eq!(get_int(&r, "key"), 42);
    }

    // TestGetInt_Float (reportutil_test.go:43).
    #[test]
    fn get_int_float() {
        let r = report(vec![("key", GoValue::Float(42.0))]);
        assert_eq!(get_int(&r, "key"), 42);
    }

    // TestGetInt_Missing (reportutil_test.go:52).
    #[test]
    fn get_int_missing() {
        let r = report(vec![]);
        assert_eq!(get_int(&r, "key"), 0);
    }

    // TestGetString_Present (reportutil_test.go:61).
    #[test]
    fn get_string_present() {
        let r = report(vec![("key", GoValue::Str("hello".into()))]);
        assert_eq!(get_string(&r, "key"), "hello");
    }

    // TestGetString_Missing (reportutil_test.go:70).
    #[test]
    fn get_string_missing() {
        let r = report(vec![]);
        assert_eq!(get_string(&r, "key"), "");
    }

    // TestGetString_WrongType (reportutil_test.go:79).
    #[test]
    fn get_string_wrong_type() {
        let r = report(vec![("key", GoValue::Int(42))]);
        assert_eq!(get_string(&r, "key"), "");
    }

    // TestGetFunctions_Present (reportutil_test.go:88).
    #[test]
    fn get_functions_present() {
        let mut fn0 = GoMap::new(MapOrigin::Map);
        fn0.insert("name", GoValue::Str("foo".into()));
        let r = report(vec![("functions", GoValue::Array(vec![GoValue::Map(fn0)]))]);
        let got = get_functions(&r, "functions");
        assert_eq!(got.len(), 1);
    }

    // TestGetFunctions_Missing (reportutil_test.go:100).
    #[test]
    fn get_functions_missing() {
        let r = report(vec![]);
        let got = get_functions(&r, "functions");
        assert!(got.is_empty());
    }

    // TestGetStringSlice_Present (reportutil_test.go:111).
    #[test]
    fn get_string_slice_present() {
        let r = report(vec![(
            "imports",
            GoValue::Array(vec![GoValue::Str("os".into()), GoValue::Str("fmt".into())]),
        )]);
        let got = get_string_slice(&r, "imports");
        assert_eq!(got.len(), 2);
    }

    // TestGetStringSlice_Missing (reportutil_test.go:122).
    #[test]
    fn get_string_slice_missing() {
        let r = report(vec![]);
        let got = get_string_slice(&r, "imports");
        assert!(got.is_empty());
    }

    // TestGetStringIntMap_Present (reportutil_test.go:133).
    #[test]
    fn get_string_int_map_present() {
        let mut counts = GoMap::new(MapOrigin::Map);
        counts.insert("os", GoValue::Int(3));
        let r = report(vec![("counts", GoValue::Map(counts))]);
        let got = get_string_int_map(&r, "counts");
        assert_eq!(
            got.iter().find(|(k, _)| k == "os").map(|(_, v)| *v),
            Some(3)
        );
    }

    // TestGetStringIntMap_Missing (reportutil_test.go:144).
    #[test]
    fn get_string_int_map_missing() {
        let r = report(vec![]);
        let got = get_string_int_map(&r, "counts");
        assert!(got.is_empty());
    }

    // TestMapString_Present (reportutil_test.go:155).
    #[test]
    fn map_string_present() {
        let m = report(vec![("name", GoValue::Str("foo".into()))]);
        assert_eq!(map_string(&m, "name"), "foo");
    }

    // TestMapString_Missing (reportutil_test.go:164).
    #[test]
    fn map_string_missing() {
        let m = report(vec![]);
        assert_eq!(map_string(&m, "name"), "");
    }

    // TestGetAs_Hit (reportutil_test.go:213) — expressed via get + match.
    #[test]
    fn get_as_hit() {
        let r = report(vec![("key", GoValue::Str("value".into()))]);
        match get(&r, "key") {
            Some(GoValue::Str(s)) => assert_eq!(s, "value"),
            other => panic!("want Str(\"value\"), got {other:?}"),
        }
    }

    // TestGetAs_KeyMissing (reportutil_test.go:229).
    #[test]
    fn get_as_key_missing() {
        let r = report(vec![]);
        assert!(get(&r, "key").is_none());
    }

    // TestGetAs_WrongType (reportutil_test.go:245).
    #[test]
    fn get_as_wrong_type() {
        let r = report(vec![("key", GoValue::Int(42))]);
        // Present but not a string → string accessor yields zero value.
        assert_eq!(get_string(&r, "key"), "");
    }

    // TestGetAs_SliceType (reportutil_test.go:261).
    #[test]
    fn get_as_slice_type() {
        let r = report(vec![(
            "imports",
            GoValue::Array(vec![GoValue::Str("os".into()), GoValue::Str("fmt".into())]),
        )]);
        let got = get_string_slice(&r, "imports");
        assert_eq!(got.len(), 2);
    }

    // TestGetAs_MapType (reportutil_test.go:277).
    #[test]
    fn get_as_map_type() {
        let mut counts = GoMap::new(MapOrigin::Map);
        counts.insert("os", GoValue::Int(3));
        let r = report(vec![("counts", GoValue::Map(counts))]);
        let got = get_string_int_map(&r, "counts");
        assert_eq!(
            got.iter().find(|(k, _)| k == "os").map(|(_, v)| *v),
            Some(3)
        );
    }
}

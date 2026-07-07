//! Type-safe accessors for dynamic report maps.
//!
//! The dynamic report is a [`cf_gojson::GoMap`] of [`cf_gojson::GoValue`]s
//! (the same model the serializer consumes); these accessors extract typed
//! scalars/collections from that tree.
//!
//! `get_float64` / `get_int` delegate to [`cf_safeconv`] for the exact numeric
//! coercion rules. The string/slice/map accessors are strict type matches: a
//! present-but-wrong-typed value yields the zero value (report accessor
//! contract — wrong types are never coerced, only numerics are).

use cf_gojson::{GoMap, GoValue};
use cf_safeconv::{Number, Value};

/// Bridges a [`GoValue`] to the dynamic [`cf_safeconv::Value`] used by the
/// numeric extractors.
///
/// Only the numeric / string / bool / null kinds participate. Containers
/// (array, map) bridge to [`Value::Nil`] so they are treated as non-numeric
/// (extraction yields `ok == false` for them).
fn to_safeconv_value(v: &GoValue) -> Value {
    match v {
        GoValue::Int(i) => Value::Number(Number::Int64(*i)),
        GoValue::Uint(u) => Value::Number(Number::Uint64(*u)),
        GoValue::Float(f) => Value::Number(Number::Float64(*f)),
        GoValue::Str(s) => Value::String(s.clone()),
        GoValue::Bool(b) => Value::Bool(*b),
        GoValue::Null | GoValue::NilSlice | GoValue::Array(_) | GoValue::Map(_) => Value::Nil,
    }
}

/// Looks up a key in a report map, returning the raw value if present.
///
/// The typed accessors below (`get_string`, `get_string_slice`, …) build on
/// this raw lookup. Key comparison is exact.
#[must_use]
pub fn get<'a>(report: &'a GoMap, key: &str) -> Option<&'a GoValue> {
    report.get(key)
}

/// Returns an `f64` from the report, coercing across numeric types.
///
/// Delegates to [`cf_safeconv::to_float64`] for consistent type handling. A
/// missing key or non-coercible value yields `0.0`.
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
/// key or non-coercible value yields `0`.
///
/// Note: `cf_safeconv::to_int` returns the word-sized `isize`; this accessor
/// widens it to `i64` to give a fixed return type on every target while
/// matching the value on the supported 64-bit hosts.
///
/// # Examples
///
/// ```
/// use cf_reportutil::{GoMap, GoValue};
/// use cf_reportutil::accessors::get_int;
///
/// let mut r = GoMap::new_map();
/// r.insert("n", GoValue::Int(42));
/// r.insert("f", GoValue::Float(42.0)); // numeric coercion across types
/// assert_eq!(get_int(&r, "n"), 42);
/// assert_eq!(get_int(&r, "f"), 42);
/// // A missing key yields 0.
/// assert_eq!(get_int(&r, "missing"), 0);
/// ```
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
/// A missing key, or a present value that is not a string, yields `""`
/// (strict type match — numbers are not stringified).
///
/// # Examples
///
/// ```
/// use cf_reportutil::{GoMap, GoValue};
/// use cf_reportutil::accessors::get_string;
///
/// let mut r = GoMap::new_map();
/// r.insert("name", GoValue::Str("hello".into()));
/// r.insert("count", GoValue::Int(42));
/// assert_eq!(get_string(&r, "name"), "hello");
/// // A present-but-wrong-typed value yields the empty string (no coercion).
/// assert_eq!(get_string(&r, "count"), "");
/// // A missing key yields the empty string.
/// assert_eq!(get_string(&r, "missing"), "");
/// ```
#[must_use]
pub fn get_string(report: &GoMap, key: &str) -> String {
    match get(report, key) {
        Some(GoValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Returns a `Vec<String>` from the report.
///
/// A missing key, or a value that is not an array of strings, yields an empty
/// vector. A *mixed* array (one element not a string) is not an array of
/// strings and yields empty (all-or-nothing; report accessor contract).
///
/// # Examples
///
/// ```
/// use cf_reportutil::{GoMap, GoValue};
/// use cf_reportutil::accessors::get_string_slice;
///
/// let mut r = GoMap::new_map();
/// r.insert("imports", GoValue::Array(vec![
///     GoValue::Str("os".into()),
///     GoValue::Str("fmt".into()),
/// ]));
/// // A mixed array (one non-string element) yields empty: all-or-nothing.
/// r.insert("mixed", GoValue::Array(vec![
///     GoValue::Str("os".into()),
///     GoValue::Int(1),
/// ]));
/// assert_eq!(get_string_slice(&r, "imports"), vec!["os", "fmt"]);
/// assert!(get_string_slice(&r, "mixed").is_empty());
/// assert!(get_string_slice(&r, "missing").is_empty());
/// ```
#[must_use]
pub fn get_string_slice(report: &GoMap, key: &str) -> Vec<String> {
    match get(report, key) {
        Some(GoValue::Array(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                if let GoValue::Str(s) = item {
                    out.push(s.clone());
                } else {
                    // Mixed array → not an array of strings → empty.
                    return Vec::new();
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

/// Returns the array of objects for the given key as borrowed maps.
///
/// Typed collections in a parsed/serialized report expose this flattened
/// array-of-objects form. A missing key, or a value that is not an array of
/// objects, yields an empty vector (all-or-nothing, like
/// [`get_string_slice`]).
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

/// Returns a string→integer object for the given key as ordered `(key, i64)`
/// pairs.
///
/// The value must be an object whose every value is an integer. A missing key,
/// or any non-integer member, yields an empty vector. Entries are returned in
/// encode order (byte-sorted for map-origin maps).
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

/// Returns a string from a map. Alias of [`get_string`], named for call sites
/// reading from an arbitrary (non-report) map.
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

    // Reference suite: TestGetFloat64_Float.
    #[test]
    fn get_float64_float() {
        let r = report(vec![("key", GoValue::Float(3.14))]);
        assert_eq!(get_float64(&r, "key"), 3.14);
    }

    // Reference suite: TestGetFloat64_Int.
    #[test]
    fn get_float64_int() {
        let r = report(vec![("key", GoValue::Int(5))]);
        assert_eq!(get_float64(&r, "key"), 5.0);
    }

    // Reference suite: TestGetFloat64_Missing.
    #[test]
    fn get_float64_missing() {
        let r = report(vec![]);
        assert_eq!(get_float64(&r, "key"), 0.0);
    }

    // Reference suite: TestGetInt_Int.
    #[test]
    fn get_int_int() {
        let r = report(vec![("key", GoValue::Int(42))]);
        assert_eq!(get_int(&r, "key"), 42);
    }

    // Reference suite: TestGetInt_Float.
    #[test]
    fn get_int_float() {
        let r = report(vec![("key", GoValue::Float(42.0))]);
        assert_eq!(get_int(&r, "key"), 42);
    }

    // Reference suite: TestGetInt_Missing.
    #[test]
    fn get_int_missing() {
        let r = report(vec![]);
        assert_eq!(get_int(&r, "key"), 0);
    }

    // Reference suite: TestGetString_Present.
    #[test]
    fn get_string_present() {
        let r = report(vec![("key", GoValue::Str("hello".into()))]);
        assert_eq!(get_string(&r, "key"), "hello");
    }

    // Reference suite: TestGetString_Missing.
    #[test]
    fn get_string_missing() {
        let r = report(vec![]);
        assert_eq!(get_string(&r, "key"), "");
    }

    // Reference suite: TestGetString_WrongType.
    #[test]
    fn get_string_wrong_type() {
        let r = report(vec![("key", GoValue::Int(42))]);
        assert_eq!(get_string(&r, "key"), "");
    }

    // Reference suite: TestGetFunctions_Present.
    #[test]
    fn get_functions_present() {
        let mut fn0 = GoMap::new(MapOrigin::Map);
        fn0.insert("name", GoValue::Str("foo".into()));
        let r = report(vec![("functions", GoValue::Array(vec![GoValue::Map(fn0)]))]);
        let got = get_functions(&r, "functions");
        assert_eq!(got.len(), 1);
    }

    // Reference suite: TestGetFunctions_Missing.
    #[test]
    fn get_functions_missing() {
        let r = report(vec![]);
        let got = get_functions(&r, "functions");
        assert!(got.is_empty());
    }

    // Reference suite: TestGetStringSlice_Present.
    #[test]
    fn get_string_slice_present() {
        let r = report(vec![(
            "imports",
            GoValue::Array(vec![GoValue::Str("os".into()), GoValue::Str("fmt".into())]),
        )]);
        let got = get_string_slice(&r, "imports");
        assert_eq!(got.len(), 2);
    }

    // Reference suite: TestGetStringSlice_Missing.
    #[test]
    fn get_string_slice_missing() {
        let r = report(vec![]);
        let got = get_string_slice(&r, "imports");
        assert!(got.is_empty());
    }

    // Reference suite: TestGetStringIntMap_Present.
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

    // Reference suite: TestGetStringIntMap_Missing.
    #[test]
    fn get_string_int_map_missing() {
        let r = report(vec![]);
        let got = get_string_int_map(&r, "counts");
        assert!(got.is_empty());
    }

    // Reference suite: TestMapString_Present.
    #[test]
    fn map_string_present() {
        let m = report(vec![("name", GoValue::Str("foo".into()))]);
        assert_eq!(map_string(&m, "name"), "foo");
    }

    // Reference suite: TestMapString_Missing.
    #[test]
    fn map_string_missing() {
        let m = report(vec![]);
        assert_eq!(map_string(&m, "name"), "");
    }

    // Reference suite: TestGetAs_Hit — expressed via get + match.
    #[test]
    fn get_as_hit() {
        let r = report(vec![("key", GoValue::Str("value".into()))]);
        match get(&r, "key") {
            Some(GoValue::Str(s)) => assert_eq!(s, "value"),
            other => panic!("want Str(\"value\"), got {other:?}"),
        }
    }

    // Reference suite: TestGetAs_KeyMissing.
    #[test]
    fn get_as_key_missing() {
        let r = report(vec![]);
        assert!(get(&r, "key").is_none());
    }

    // Reference suite: TestGetAs_WrongType.
    #[test]
    fn get_as_wrong_type() {
        let r = report(vec![("key", GoValue::Int(42))]);
        // Present but not a string → string accessor yields zero value.
        assert_eq!(get_string(&r, "key"), "");
    }

    // Reference suite: TestGetAs_SliceType.
    #[test]
    fn get_as_slice_type() {
        let r = report(vec![(
            "imports",
            GoValue::Array(vec![GoValue::Str("os".into()), GoValue::Str("fmt".into())]),
        )]);
        let got = get_string_slice(&r, "imports");
        assert_eq!(got.len(), 2);
    }

    // Reference suite: TestGetAs_MapType.
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

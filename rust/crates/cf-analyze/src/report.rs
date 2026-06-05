//! The core [`Report`] type.
//!
//! In Go this is `type Report = map[string]any` (analyzer.go:26). The dominant
//! ordering rule for every machine format is therefore Go's `map[string]X`
//! byte-sorted key ordering, which [`cf_gojson::GoMap`] reproduces. We model a
//! `Report` as a **map-origin** [`GoMap`] so that, on encode, keys are sorted
//! by `key.as_bytes()` exactly as Go does.
//!
//! Helper accessors that mirror the Go free functions
//! (`ReportFunctionList`, `ReportFunctionListWithFallback`) operate over the
//! same value model.

use cf_gojson::{GoMap, GoValue};

/// Analysis output: a string-keyed map of arbitrary JSON values.
///
/// Port of the Go `Report = map[string]any` alias. Backed by a map-origin
/// [`GoMap`] so machine-format serialization byte-sorts keys like Go.
pub type Report = GoMap;

/// Creates an empty report (map-origin, keys byte-sorted on encode).
pub fn new_report() -> Report {
    GoMap::new_map()
}

/// Returns a reference to the value stored under `key`, if present.
pub fn report_get<'a>(report: &'a Report, key: &str) -> Option<&'a GoValue> {
    report.entries().iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Extracts a list of objects from a report key.
///
/// Port of Go `ReportFunctionList`. Handles the JSON-decoded `[]any` slice
/// case: an array value whose elements are objects yields those objects.
///
/// The Go version additionally special-cases the `TypedCollection` wrapper and
/// a directly-typed `[]map[string]any`; those typed cases collapse to the same
/// array-of-objects representation here because [`GoValue`] is the post-decode
/// model. Returns `None` when the key is absent or holds no object elements.
pub fn report_function_list<'a>(report: &'a Report, key: &str) -> Option<Vec<&'a GoMap>> {
    let val = report_get(report, key)?;
    let arr = match val {
        GoValue::Array(a) => a,
        _ => return None,
    };
    let mut result: Vec<&GoMap> = Vec::with_capacity(arr.len());
    for item in arr {
        if let GoValue::Map(m) = item {
            result.push(m);
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Extracts a function list trying `primary_key` first, then `fallback_key`.
/// Port of Go `ReportFunctionListWithFallback`.
pub fn report_function_list_with_fallback<'a>(
    report: &'a Report,
    primary_key: &str,
    fallback_key: &str,
) -> Option<Vec<&'a GoMap>> {
    report_function_list(report, primary_key)
        .or_else(|| report_function_list(report, fallback_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::GoValue;

    #[test]
    fn function_list_from_array_of_objects() {
        let mut inner = GoMap::new_map();
        inner.push("a", GoValue::Int(1));
        let mut r = new_report();
        r.push("funcs", GoValue::Array(vec![GoValue::Object(inner)]));
        let got = report_function_list(&r, "funcs").unwrap();
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn function_list_missing_key() {
        let r = new_report();
        assert!(report_function_list(&r, "nope").is_none());
    }

    #[test]
    fn function_list_non_array() {
        let mut r = new_report();
        r.push("x", GoValue::Int(5));
        assert!(report_function_list(&r, "x").is_none());
    }

    #[test]
    fn fallback_uses_secondary_key() {
        let mut inner = GoMap::new_map();
        inner.push("a", GoValue::Int(1));
        let mut r = new_report();
        r.push("secondary", GoValue::Array(vec![GoValue::Object(inner)]));
        let got = report_function_list_with_fallback(&r, "primary", "secondary").unwrap();
        assert_eq!(got.len(), 1);
    }
}

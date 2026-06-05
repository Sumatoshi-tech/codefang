//! Report serialization (DESIGN §2).
//!
//! All MACHINE-format report bytes (json, yaml, ndjson, timeseries, compact,
//! bin) must be byte-identical to Go. Every report-bearing value is built as a
//! [`cf_gojson::GoValue`] tree and serialized through [`cf_gojson::marshal`] /
//! [`cf_gojson::marshal_indent`], never through `serde_json` (which differs from
//! Go's `encoding/json` on map-key ordering, HTML escaping, and float
//! formatting — see DESIGN §2.1). YAML reuses the same `GoValue` tree through
//! `cf-goyaml` once that path is wired.
//!
//! Two payload shapes are produced, matching the two Go code paths:
//!
//! * [`computed_metrics_to_value`] — the `ComputedMetrics` **struct**
//!   (`file_coupling`, `developer_coupling`, `file_ownership`, `aggregate`),
//!   emitted in Go field declaration order with `omitempty` honored
//!   ([`MapOrigin::Struct`]).
//! * [`dense_report_to_value`] — the raw analyzer `Report`, a Go
//!   `map[string]any` whose keys (and the `map[int]int64` matrix rows) Go
//!   byte-sorts at encode time ([`MapOrigin::Map`]).

use crate::metrics::{
    AggregateData, ComputedMetrics, DeveloperCouplingData, FileCouplingData, FileOwnershipData,
    ReportData,
};
use cf_gojson::{marshal, marshal_indent, GoMap, GoValue};
use std::collections::BTreeMap;

/// Builds the `GoValue` tree for [`ComputedMetrics`] (struct origin).
///
/// Field order matches the Go struct declaration order; `omitempty` string
/// fields (`developer*_email`, `top_contributor`) are skipped when empty,
/// reproducing `encoding/json`.
pub fn computed_metrics_to_value(m: &ComputedMetrics) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.push(
        "file_coupling",
        GoValue::Array(m.file_coupling.iter().map(file_coupling_to_value).collect()),
    );
    obj.push(
        "developer_coupling",
        GoValue::Array(m.developer_coupling.iter().map(developer_coupling_to_value).collect()),
    );
    obj.push(
        "file_ownership",
        GoValue::Array(m.file_ownership.iter().map(file_ownership_to_value).collect()),
    );
    obj.push("aggregate", aggregate_to_value(&m.aggregate));
    GoValue::Object(obj)
}

fn file_coupling_to_value(d: &FileCouplingData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("file1", GoValue::Str(d.file1.clone()));
    o.push("file2", GoValue::Str(d.file2.clone()));
    o.push("co_changes", GoValue::Int(d.co_changes));
    o.push("coupling_strength", GoValue::Float(d.strength));
    GoValue::Object(o)
}

fn developer_coupling_to_value(d: &DeveloperCouplingData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("developer1", GoValue::Str(d.developer1.clone()));
    if !d.developer1_email.is_empty() {
        o.push("developer1_email", GoValue::Str(d.developer1_email.clone()));
    }
    o.push("developer2", GoValue::Str(d.developer2.clone()));
    if !d.developer2_email.is_empty() {
        o.push("developer2_email", GoValue::Str(d.developer2_email.clone()));
    }
    o.push("shared_file_changes", GoValue::Int(d.shared_files));
    o.push("coupling_strength", GoValue::Float(d.strength));
    GoValue::Object(o)
}

fn file_ownership_to_value(d: &FileOwnershipData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("file", GoValue::Str(d.file.clone()));
    o.push("lines", GoValue::Int(d.lines as i64));
    o.push("contributors", GoValue::Int(d.contributors as i64));
    if !d.top_contributor.is_empty() {
        o.push("top_contributor", GoValue::Str(d.top_contributor.clone()));
    }
    GoValue::Object(o)
}

fn aggregate_to_value(d: &AggregateData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("total_files", GoValue::Int(d.total_files as i64));
    o.push("total_developers", GoValue::Int(d.total_developers as i64));
    o.push("total_co_changes", GoValue::Int(d.total_co_changes));
    o.push("avg_coupling_strength", GoValue::Float(d.avg_coupling_strength));
    o.push("highly_coupled_pairs", GoValue::Int(d.highly_coupled_pairs as i64));
    GoValue::Object(o)
}

/// Builds the `GoValue` tree for the raw dense analyzer report (Go: the
/// `analyze.Report` map built by `buildReport`).
///
/// This is a Go `map[string]any`, so it is map-origin: `cf-gojson` byte-sorts
/// the keys at encode time, and insertion order here is not load-bearing.
/// Matrix rows are Go `map[int]int64`, also map-origin, rendering with
/// stringified, byte-sorted integer keys exactly like Go.
pub fn dense_report_to_value(r: &ReportData) -> GoValue {
    let mut obj = GoMap::new_map();
    obj.push("PeopleMatrix", matrix_to_value(&r.people_matrix));
    obj.push(
        "PeopleFiles",
        GoValue::Array(
            r.people_files
                .iter()
                .map(|row| GoValue::Array(row.iter().map(|&i| GoValue::Int(i as i64)).collect()))
                .collect(),
        ),
    );
    obj.push(
        "Files",
        GoValue::Array(r.files.iter().map(|s| GoValue::Str(s.clone())).collect()),
    );
    obj.push(
        "FilesLines",
        GoValue::Array(r.files_lines.iter().map(|&l| GoValue::Int(l as i64)).collect()),
    );
    obj.push("FilesMatrix", matrix_to_value(&r.files_matrix));
    obj.push(
        "ReversedPeopleDict",
        GoValue::Array(r.reversed_people_dict.iter().map(|s| GoValue::Str(s.clone())).collect()),
    );
    GoValue::Object(obj)
}

/// Converts a slice of index-keyed matrix rows to an array of map-origin
/// objects with stringified integer keys (Go `[]map[int]int64`).
fn matrix_to_value(matrix: &[BTreeMap<usize, i64>]) -> GoValue {
    GoValue::Array(
        matrix
            .iter()
            .map(|row| {
                let mut o = GoMap::new_map();
                for (idx, count) in row {
                    o.push(idx.to_string(), GoValue::Int(*count));
                }
                GoValue::Object(o)
            })
            .collect(),
    )
}

/// Serializes a `GoValue` to compact Go-JSON bytes (mirrors `json.Marshal`:
/// no insignificant whitespace, HTML escaping on, no trailing newline).
pub fn to_go_json(value: &GoValue) -> Vec<u8> {
    marshal(value)
}

/// Serializes a `GoValue` to two-space-indented Go-JSON bytes (mirrors
/// `json.Encoder` with `SetIndent("", "  ")`; the caller appends the trailing
/// newline the run/render path emits).
pub fn to_go_json_indent(value: &GoValue) -> Vec<u8> {
    marshal_indent(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computed_metrics_field_order_and_omitempty() {
        let m = ComputedMetrics {
            developer_coupling: vec![DeveloperCouplingData {
                developer1: "Alice".into(),
                developer1_email: String::new(), // omitted.
                developer2: "Bob".into(),
                developer2_email: "b@x".into(),
                shared_files: 3,
                strength: 0.5,
            }],
            aggregate: AggregateData { total_files: 1, ..Default::default() },
            ..Default::default()
        };
        let v = computed_metrics_to_value(&m);
        let GoValue::Map(obj) = v else { panic!("expected object") };
        // struct-origin: declaration order preserved.
        let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            ["file_coupling", "developer_coupling", "file_ownership", "aggregate"]
        );
        let dc_arr = obj.get("developer_coupling").unwrap();
        let GoValue::Array(items) = dc_arr else { panic!("expected array") };
        let GoValue::Map(dc) = &items[0] else { panic!("expected object") };
        assert!(!dc.contains_key("developer1_email")); // omitempty, empty.
        assert_eq!(dc.get("developer2_email"), Some(&GoValue::Str("b@x".into())));
        assert_eq!(dc.get("shared_file_changes"), Some(&GoValue::Int(3)));
    }

    #[test]
    fn computed_metrics_json_bytes_compact() {
        let m = ComputedMetrics {
            file_coupling: vec![FileCouplingData {
                file1: "a.go".into(),
                file2: "b.go".into(),
                co_changes: 2,
                strength: 0.5,
            }],
            aggregate: AggregateData {
                total_files: 2,
                total_developers: 1,
                total_co_changes: 2,
                avg_coupling_strength: 0.5,
                highly_coupled_pairs: 0,
            },
            ..Default::default()
        };
        let bytes = to_go_json(&computed_metrics_to_value(&m));
        let s = String::from_utf8(bytes).unwrap();
        // Struct field declaration order, compact (no spaces), Go float "0.5".
        assert!(s.starts_with(r#"{"file_coupling":[{"file1":"a.go","file2":"b.go","co_changes":2,"coupling_strength":0.5}]"#));
        assert!(s.contains(r#""aggregate":{"total_files":2,"total_developers":1,"total_co_changes":2,"avg_coupling_strength":0.5,"highly_coupled_pairs":0}"#));
    }

    #[test]
    fn dense_report_map_keys_byte_sorted() {
        let r = ReportData {
            files: vec!["a.go".into()],
            files_lines: vec![5],
            files_matrix: vec![BTreeMap::from([(0usize, 4i64)])],
            people_matrix: vec![BTreeMap::from([(0usize, 2i64)])],
            people_files: vec![vec![0]],
            reversed_people_dict: vec!["Alice|a".into()],
        };
        let bytes = to_go_json(&dense_report_to_value(&r));
        let s = String::from_utf8(bytes).unwrap();
        // map[string]any keys byte-sorted: Files < FilesLines < FilesMatrix <
        // PeopleFiles < PeopleMatrix < ReversedPeopleDict.
        assert!(s.starts_with(r#"{"Files":["a.go"],"FilesLines":[5],"FilesMatrix":[{"0":4}]"#));
        // matrix row map[int]int64 stringified key.
        assert!(s.contains(r#""PeopleMatrix":[{"0":2}]"#));
    }
}

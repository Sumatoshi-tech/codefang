//! Report serialization.
//!
//! The report is built as a [`cf_gojson::GoValue`] tree
//! ([`computed_metrics_to_value`]) and serialized through `cf-gojson` /
//! `cf-goyaml` by the command layer, never through `serde_json` (which
//! differs from the report contract on map-key ordering, HTML escaping and
//! float formatting). The [`ComputedMetrics`] struct is emitted in
//! declaration order (`file_coupling`, `developer_coupling`,
//! `file_ownership`, `aggregate`) with omit-when-empty honored.

use crate::metrics::{
    AggregateData, ComputedMetrics, DeveloperCouplingData, FileCouplingData, FileOwnershipData,
};
use cf_gojson::{GoMap, GoValue};

/// Builds the [`GoValue`] tree for [`ComputedMetrics`] (struct origin).
///
/// Field order is the contractual declaration order; omit-when-empty string
/// fields (`developer*_email`, `top_contributor`) are skipped when empty.
#[must_use]
pub fn computed_metrics_to_value(m: &ComputedMetrics) -> GoValue {
    let mut obj = GoMap::new_struct();
    // `file_coupling` renders as JSON `null` (YAML `[]`) when no pair
    // co-changed, never `[]`.
    obj.push(
        "file_coupling",
        if m.file_coupling.is_empty() {
            GoValue::NilSlice
        } else {
            GoValue::Array(m.file_coupling.iter().map(file_coupling_to_value).collect())
        },
    );
    obj.push(
        "developer_coupling",
        GoValue::Array(
            m.developer_coupling
                .iter()
                .map(developer_coupling_to_value)
                .collect(),
        ),
    );
    obj.push(
        "file_ownership",
        GoValue::Array(
            m.file_ownership
                .iter()
                .map(file_ownership_to_value)
                .collect(),
        ),
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
    o.push("lines", GoValue::Int(i64::from(d.lines)));
    o.push("contributors", GoValue::Int(i64::from(d.contributors)));
    if !d.top_contributor.is_empty() {
        o.push("top_contributor", GoValue::Str(d.top_contributor.clone()));
    }
    GoValue::Object(o)
}

fn aggregate_to_value(d: &AggregateData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("total_files", GoValue::Int(i64::from(d.total_files)));
    o.push(
        "total_developers",
        GoValue::Int(i64::from(d.total_developers)),
    );
    o.push("total_co_changes", GoValue::Int(d.total_co_changes));
    o.push(
        "avg_coupling_strength",
        GoValue::Float(d.avg_coupling_strength),
    );
    o.push(
        "highly_coupled_pairs",
        GoValue::Int(i64::from(d.highly_coupled_pairs)),
    );
    GoValue::Object(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gojson::marshal;

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
            aggregate: AggregateData {
                total_files: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let v = computed_metrics_to_value(&m);
        let GoValue::Map(obj) = v else {
            panic!("expected object")
        };
        // struct-origin: declaration order preserved.
        let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "file_coupling",
                "developer_coupling",
                "file_ownership",
                "aggregate"
            ]
        );
        let dc_arr = obj.get("developer_coupling").unwrap();
        let GoValue::Array(items) = dc_arr else {
            panic!("expected array")
        };
        let GoValue::Map(dc) = &items[0] else {
            panic!("expected object")
        };
        assert!(!dc.contains_key("developer1_email")); // omitempty, empty.
        assert_eq!(
            dc.get("developer2_email"),
            Some(&GoValue::Str("b@x".into()))
        );
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
        let bytes = marshal(&computed_metrics_to_value(&m));
        let s = String::from_utf8(bytes).unwrap();
        // Struct field declaration order, compact (no spaces), float "0.5".
        assert!(s.starts_with(r#"{"file_coupling":[{"file1":"a.go","file2":"b.go","co_changes":2,"coupling_strength":0.5}]"#));
        assert!(s.contains(r#""aggregate":{"total_files":2,"total_developers":1,"total_co_changes":2,"avg_coupling_strength":0.5,"highly_coupled_pairs":0}"#));
    }
}

//! Machine-format report assembly for the shotness analyzer.
//!
//! The serialized machine output of the shotness analyzer is its
//! [`crate::metrics::ComputedMetrics`]. This module assembles
//! the ordered [`GoValue`] tree so cf-gojson renders the contractual report
//! bytes (pinned by the report e2e suites (`bins/codefang/tests`)):
//!
//! - [`ComputedMetrics`] is struct-origin → keys in declaration order
//!   (`node_hotness`, `node_coupling`, `hotspot_nodes`, `aggregate`);
//! - each row struct emits its fields in declaration order.
//!
//! All serialization routes through cf-gojson; never serde.

use cf_gojson::{GoMap, GoValue};

use crate::metrics::{
    AggregateData, ComputedMetrics, HotspotNodeData, NodeCouplingData, NodeHotnessData,
};

fn node_hotness_to_value(h: &NodeHotnessData, ended_tracked: bool) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("name", GoValue::Str(h.name.clone()));
    o.push("type", GoValue::Str(h.type_.clone()));
    o.push("file", GoValue::Str(h.file.clone()));
    o.push("change_count", GoValue::Int(h.change_count));
    o.push("coupled_nodes", GoValue::Int(h.coupled_nodes));
    o.push("hotness_score", GoValue::Float(h.hotness_score));
    if ended_tracked {
        o.push("ended", GoValue::Bool(h.ended));
    }
    GoValue::Map(o)
}

fn node_coupling_to_value(c: &NodeCouplingData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("node1_name", GoValue::Str(c.node1_name.clone()));
    o.push("node1_file", GoValue::Str(c.node1_file.clone()));
    o.push("node2_name", GoValue::Str(c.node2_name.clone()));
    o.push("node2_file", GoValue::Str(c.node2_file.clone()));
    o.push("co_changes", GoValue::Int(c.co_changes));
    o.push("coupling_strength", GoValue::Float(c.strength));
    GoValue::Map(o)
}

fn hotspot_to_value(h: &HotspotNodeData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("name", GoValue::Str(h.name.clone()));
    o.push("type", GoValue::Str(h.type_.clone()));
    o.push("file", GoValue::Str(h.file.clone()));
    o.push("change_count", GoValue::Int(h.change_count));
    o.push("risk_level", GoValue::Str(h.risk_level.clone()));
    GoValue::Map(o)
}

/// Builds the [`GoValue`] tree for [`AggregateData`] (struct field order).
fn aggregate_to_value(a: &AggregateData, ended_tracked: bool) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("total_nodes", GoValue::Int(a.total_nodes));
    o.push("total_changes", GoValue::Int(a.total_changes));
    o.push("total_couplings", GoValue::Int(a.total_couplings));
    o.push(
        "avg_changes_per_node",
        GoValue::Float(a.avg_changes_per_node),
    );
    o.push(
        "avg_coupling_strength",
        GoValue::Float(a.avg_coupling_strength),
    );
    o.push("hot_nodes", GoValue::Int(a.hot_nodes));
    if ended_tracked {
        o.push("ended_nodes", GoValue::Int(a.ended_nodes));
    }
    GoValue::Map(o)
}

impl ComputedMetrics {
    /// Builds the ordered [`GoValue`] tree for the machine-format report
    /// (struct declaration order).
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut root = GoMap::new_struct();

        root.push(
            "node_hotness",
            GoValue::Array(
                self.node_hotness
                    .iter()
                    .map(|h| node_hotness_to_value(h, self.ended_tracked))
                    .collect(),
            ),
        );
        // Reference asymmetry: `node_hotness` is built with `make(..., 0, n)`
        // (never nil ⇒ `[]` even when empty), while `node_coupling` and
        // `hotspot_nodes` are `var`-declared append targets — zero appends
        // leave them nil, which encodes as JSON `null` / YAML `[]`.
        root.push(
            "node_coupling",
            if self.node_coupling.is_empty() {
                GoValue::NilSlice
            } else {
                GoValue::Array(
                    self.node_coupling
                        .iter()
                        .map(node_coupling_to_value)
                        .collect(),
                )
            },
        );
        root.push(
            "hotspot_nodes",
            if self.hotspot_nodes.is_empty() {
                GoValue::NilSlice
            } else {
                GoValue::Array(self.hotspot_nodes.iter().map(hotspot_to_value).collect())
            },
        );
        root.push(
            "aggregate",
            aggregate_to_value(&self.aggregate, self.ended_tracked),
        );

        GoValue::Map(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::compute_all_metrics;
    use crate::types::{NodeSummary, ReportData};
    use cf_gojson::marshal::marshal;
    use std::collections::HashMap;

    fn hm(pairs: &[(usize, i64)]) -> HashMap<usize, i64> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn empty_metrics_struct_order() {
        let m = ComputedMetrics::default();
        let bytes = marshal(&m.to_go_value());
        assert_eq!(
            bytes,
            br#"{"node_hotness":[],"node_coupling":null,"hotspot_nodes":null,"aggregate":{"total_nodes":0,"total_changes":0,"total_couplings":0,"avg_changes_per_node":0,"avg_coupling_strength":0,"hot_nodes":0}}"#
        );
    }

    #[test]
    fn two_node_report_struct_field_order_and_hottest_first() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("Function", "bar", "a.go"),
                NodeSummary::new("Function", "foo", "a.go"),
            ],
            counters: vec![hm(&[(0, 6), (1, 3)]), hm(&[(1, 10), (0, 3)])],
            ended: Vec::new(),
            ended_tracked: false,
        };
        let metrics = compute_all_metrics(&input);
        let s = String::from_utf8(marshal(&metrics.to_go_value())).unwrap();
        assert!(s.starts_with(r#"{"node_hotness":["#));
        assert!(s.contains(r#""node_coupling":"#));
        assert!(s.contains(r#""hotspot_nodes":"#));
        assert!(s.contains(r#""aggregate":"#));
        // Hottest node (foo, change_count 10) sorts first; field order preserved.
        assert!(s.contains(r#"{"name":"foo","type":"Function","file":"a.go","change_count":10"#));
        assert!(
            s.contains(r#""hotness_score":1}"#),
            "untracked: no ended field: {s}"
        );
        assert!(!s.contains("ended_nodes"), "{s}");
    }

    #[test]
    fn a_tracked_report_carries_the_ended_fields() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("Function", "bar", "a.go"),
                NodeSummary::new("Function", "foo", "a.go"),
            ],
            counters: vec![hm(&[(0, 6), (1, 3)]), hm(&[(1, 10), (0, 3)])],
            ended: vec![true, false],
            ended_tracked: true,
        };
        let s = String::from_utf8(marshal(&compute_all_metrics(&input).to_go_value())).unwrap();
        assert!(s.contains(r#""name":"bar","type":"Function","file":"a.go","change_count":6,"coupled_nodes":1,"hotness_score":0.6,"ended":true}"#), "{s}");
        assert!(s.contains(r#""hotness_score":1,"ended":false}"#), "{s}");
        assert!(s.contains(r#""hot_nodes":1,"ended_nodes":1}"#), "{s}");
    }
}

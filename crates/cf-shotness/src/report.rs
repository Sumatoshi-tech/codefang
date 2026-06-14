//! Machine-format report assembly for the shotness analyzer.
//!
//! The serialized machine output of the shotness analyzer is its
//! [`crate::metrics::ComputedMetrics`]. This module assembles
//! the ordered [`GoValue`] tree so cf-gojson renders the contractual report
//! bytes (pinned by `tests/compat`):
//!
//! - [`ComputedMetrics`] is struct-origin → keys in declaration order
//!   (`node_hotness`, `node_coupling`, `hotspot_nodes`, `aggregate`);
//! - each row struct emits its fields in declaration order;
//! - the per-commit timeseries summary is map-origin → keys byte-sorted
//!   (`coupling_pairs` before `nodes_touched`).
//!
//! All serialization routes through cf-gojson; never serde.

use cf_gojson::{GoMap, GoValue};

use crate::metrics::{
    AggregateData, ComputedMetrics, HotspotNodeData, NodeCouplingData, NodeHotnessData,
};

/// Per-commit summary for timeseries output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    /// Number of nodes touched in the commit.
    pub nodes_touched: i64,
    /// Number of coupling pairs implied by the touched nodes.
    pub coupling_pairs: i64,
}

impl CommitSummary {
    /// JSON-friendly map representation.
    ///
    /// Built as a **map-origin** object so cf-gojson byte-sorts the keys
    /// (report-format contract): `coupling_pairs` sorts before
    /// `nodes_touched`.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_map();
        m.push("nodes_touched", GoValue::Int(self.nodes_touched));
        m.push("coupling_pairs", GoValue::Int(self.coupling_pairs));
        GoValue::Map(m)
    }
}

fn node_hotness_to_value(h: &NodeHotnessData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("name", GoValue::Str(h.name.clone()));
    o.push("type", GoValue::Str(h.type_.clone()));
    o.push("file", GoValue::Str(h.file.clone()));
    o.push("change_count", GoValue::Int(h.change_count));
    o.push("coupled_nodes", GoValue::Int(h.coupled_nodes));
    o.push("hotness_score", GoValue::Float(h.hotness_score));
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
#[must_use]
pub fn aggregate_to_value(a: &AggregateData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("total_nodes", GoValue::Int(a.total_nodes));
    o.push("total_changes", GoValue::Int(a.total_changes));
    o.push("total_couplings", GoValue::Int(a.total_couplings));
    o.push("avg_changes_per_node", GoValue::Float(a.avg_changes_per_node));
    o.push("avg_coupling_strength", GoValue::Float(a.avg_coupling_strength));
    o.push("hot_nodes", GoValue::Int(a.hot_nodes));
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
            GoValue::Array(self.node_hotness.iter().map(node_hotness_to_value).collect()),
        );
        root.push(
            "node_coupling",
            GoValue::Array(self.node_coupling.iter().map(node_coupling_to_value).collect()),
        );
        root.push(
            "hotspot_nodes",
            GoValue::Array(self.hotspot_nodes.iter().map(hotspot_to_value).collect()),
        );
        root.push("aggregate", aggregate_to_value(&self.aggregate));

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
            br#"{"node_hotness":[],"node_coupling":[],"hotspot_nodes":[],"aggregate":{"total_nodes":0,"total_changes":0,"total_couplings":0,"avg_changes_per_node":0,"avg_coupling_strength":0,"hot_nodes":0}}"#
        );
    }

    #[test]
    fn commit_summary_keys_byte_sorted() {
        let cs = CommitSummary {
            nodes_touched: 3,
            coupling_pairs: 3,
        };
        // map-origin: coupling_pairs sorts before nodes_touched.
        assert_eq!(
            marshal(&cs.to_go_value()),
            br#"{"coupling_pairs":3,"nodes_touched":3}"#
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
        };
        let metrics = compute_all_metrics(&input);
        let s = String::from_utf8(marshal(&metrics.to_go_value())).unwrap();
        assert!(s.starts_with(r#"{"node_hotness":["#));
        assert!(s.contains(r#""node_coupling":"#));
        assert!(s.contains(r#""hotspot_nodes":"#));
        assert!(s.contains(r#""aggregate":"#));
        // Hottest node (foo, change_count 10) sorts first; field order preserved.
        assert!(s.contains(r#"{"name":"foo","type":"Function","file":"a.go","change_count":10"#));
    }
}

//! Pure metric functions for the shotness analyzer.
//!
//! Direct port of `internal/analyzers/shotness/metrics.go`. Every function
//! reproduces the Go control flow, sort ordering, and arithmetic exactly so the
//! computed metrics — and therefore the machine-format report bytes — match.

use std::collections::HashMap;

use crate::types::{NodeSummary, ReportData};

/// Hotness information for a single code node. Mirrors Go `NodeHotnessData`.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeHotnessData {
    /// Node display name.
    pub name: String,
    /// Node type.
    pub type_: String,
    /// Source file.
    pub file: String,
    /// Self-change count.
    pub change_count: i64,
    /// Number of distinct coupled nodes (counter row size minus self).
    pub coupled_nodes: i64,
    /// Hotness score normalized to `[0, 1]` against the hottest node.
    pub hotness_score: f64,
}

/// Coupling between two code nodes. Mirrors Go `NodeCouplingData`.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeCouplingData {
    /// First node name.
    pub node1_name: String,
    /// First node file.
    pub node1_file: String,
    /// Second node name.
    pub node2_name: String,
    /// Second node file.
    pub node2_file: String,
    /// Number of commits in which both nodes changed.
    pub co_changes: i64,
    /// Normalized coupling strength in `[0, 1]`.
    pub strength: f64,
}

/// A hotspot node (MEDIUM or HIGH risk). Mirrors Go `HotspotNodeData`.
#[derive(Debug, Clone, PartialEq)]
pub struct HotspotNodeData {
    /// Node name.
    pub name: String,
    /// Node type.
    pub type_: String,
    /// Source file.
    pub file: String,
    /// Self-change count.
    pub change_count: i64,
    /// Risk classification (`HIGH` / `MEDIUM` / `LOW`).
    pub risk_level: String,
}

/// Summary statistics. Mirrors Go `AggregateData`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AggregateData {
    /// Total number of nodes.
    pub total_nodes: i64,
    /// Sum of all self-change counts.
    pub total_changes: i64,
    /// Number of distinct co-change pairs.
    pub total_couplings: i64,
    /// Average self-changes per node.
    pub avg_changes_per_node: f64,
    /// Average coupling strength across all pairs.
    pub avg_coupling_strength: f64,
    /// Number of nodes at MEDIUM risk or above.
    pub hot_nodes: i64,
}

/// All computed metric results. Mirrors Go `ComputedMetrics`.
///
/// Field declaration order is load-bearing: the machine-format JSON/YAML emits
/// `node_hotness`, `node_coupling`, `hotspot_nodes`, `aggregate` in this order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ComputedMetrics {
    /// Per-node hotness rows, sorted by change count descending.
    pub node_hotness: Vec<NodeHotnessData>,
    /// Coupling rows, sorted by co-change count descending.
    pub node_coupling: Vec<NodeCouplingData>,
    /// Hotspot rows (MEDIUM/HIGH), sorted by change count descending.
    pub hotspot_nodes: Vec<HotspotNodeData>,
    /// Aggregate statistics.
    pub aggregate: AggregateData,
}

/// Self-change threshold for HIGH risk. Mirrors `HotspotThresholdHigh`.
pub const HOTSPOT_THRESHOLD_HIGH: i64 = 20;
/// Self-change threshold for MEDIUM risk. Mirrors `HotspotThresholdMedium`.
pub const HOTSPOT_THRESHOLD_MEDIUM: i64 = 10;

/// Risk level label: HIGH. Mirrors `RiskLevelHigh`.
pub const RISK_LEVEL_HIGH: &str = "HIGH";
/// Risk level label: MEDIUM. Mirrors `RiskLevelMedium`.
pub const RISK_LEVEL_MEDIUM: &str = "MEDIUM";
/// Risk level label: LOW. Mirrors `RiskLevelLow`.
pub const RISK_LEVEL_LOW: &str = "LOW";

/// Looks up `row[idx]`, returning 0 when absent (Go map zero-value semantics).
fn counter_at(row: &HashMap<usize, i64>, idx: usize) -> i64 {
    row.get(&idx).copied().unwrap_or(0)
}

/// Computes per-node hotness data.
///
/// Port of Go `computeNodeHotness`. Normalizes the self-change count against the
/// maximum self-change count across all nodes and sorts by change count
/// descending.
#[must_use]
pub fn compute_node_hotness(input: &ReportData) -> Vec<NodeHotnessData> {
    let mut result: Vec<NodeHotnessData> = Vec::with_capacity(input.nodes.len());

    // Find max change count for normalization.
    let mut max_changes: i64 = 0;
    for (i, counters) in input.counters.iter().enumerate() {
        if let Some(&self_count) = counters.get(&i) {
            if self_count > max_changes {
                max_changes = self_count;
            }
        }
    }

    for (i, node) in input.nodes.iter().enumerate() {
        if i >= input.counters.len() {
            continue;
        }

        let counters = &input.counters[i];
        let change_count = counter_at(counters, i);
        let coupled_nodes = counters.len() as i64 - 1; // Exclude self.

        let hotness_score = if max_changes > 0 {
            change_count as f64 / max_changes as f64
        } else {
            0.0
        };

        result.push(NodeHotnessData {
            name: node.name.clone(),
            type_: node.type_.clone(),
            file: node.file.clone(),
            change_count,
            coupled_nodes,
            hotness_score,
        });
    }

    sort_by_desc_stable(&mut result, |r| r.change_count);

    result
}

/// Computes coupling rows with normalized strength.
///
/// Port of Go `computeNodeCoupling`. Iterates the upper triangle of the
/// co-change matrix (`j > i`), skipping zero co-changes, and sorts by co-change
/// count descending.
#[must_use]
pub fn compute_node_coupling(input: &ReportData) -> Vec<NodeCouplingData> {
    let mut result: Vec<NodeCouplingData> = Vec::new();

    for (i, counters) in input.counters.iter().enumerate() {
        if i >= input.nodes.len() {
            continue;
        }

        let node1 = &input.nodes[i];
        let self_changes_i = counter_at(counters, i);

        // Deterministic upper-triangle walk over j in [i+1, nodes).
        for j in (i + 1)..input.nodes.len() {
            let co_changes = counter_at(counters, j);
            if co_changes == 0 {
                continue;
            }

            let node2 = &input.nodes[j];

            let self_changes_j = if j < input.counters.len() {
                counter_at(&input.counters[j], j)
            } else {
                0
            };

            let strength = compute_coupling_strength(co_changes, self_changes_i, self_changes_j);

            result.push(NodeCouplingData {
                node1_name: node1.name.clone(),
                node1_file: node1.file.clone(),
                node2_name: node2.name.clone(),
                node2_file: node2.file.clone(),
                co_changes,
                strength,
            });
        }
    }

    sort_by_desc_stable(&mut result, |r| r.co_changes);

    result
}

/// Normalized coupling confidence in `[0, 1]`.
///
/// Port of Go `computeCouplingStrength`. Formula:
/// `co_changes / max(co_changes, changes_a, changes_b)`. Including `co_changes`
/// in the denominator guarantees the result never exceeds 1.
#[must_use]
pub fn compute_coupling_strength(co_changes: i64, changes_a: i64, changes_b: i64) -> f64 {
    let max_changes = co_changes.max(changes_a.max(changes_b));
    if max_changes <= 0 {
        return 0.0;
    }
    co_changes as f64 / max_changes as f64
}

/// Classifies a self-change count into a risk level.
///
/// Mirrors the Go `changeRiskClassifier` (thresholds sorted descending; returns
/// the first whose limit is `<=` the value): HIGH≥20, MEDIUM≥10, else LOW.
#[must_use]
pub fn classify_change_risk(change_count: i64) -> &'static str {
    if change_count >= HOTSPOT_THRESHOLD_HIGH {
        RISK_LEVEL_HIGH
    } else if change_count >= HOTSPOT_THRESHOLD_MEDIUM {
        RISK_LEVEL_MEDIUM
    } else {
        RISK_LEVEL_LOW
    }
}

/// Computes hotspot nodes (MEDIUM and HIGH risk only).
///
/// Port of Go `computeHotspotNodes`.
#[must_use]
pub fn compute_hotspot_nodes(input: &ReportData) -> Vec<HotspotNodeData> {
    let mut result: Vec<HotspotNodeData> = Vec::new();

    for (i, n) in input.nodes.iter().enumerate() {
        if i >= input.counters.len() {
            continue;
        }

        let counters = &input.counters[i];
        let change_count = counter_at(counters, i);

        let risk_level = classify_change_risk(change_count);
        if risk_level == RISK_LEVEL_LOW {
            continue;
        }

        result.push(HotspotNodeData {
            name: n.name.clone(),
            type_: n.type_.clone(),
            file: n.file.clone(),
            change_count,
            risk_level: risk_level.to_string(),
        });
    }

    sort_by_desc_stable(&mut result, |r| r.change_count);

    result
}

/// Computes aggregate statistics.
///
/// Port of Go `computeAggregate`. The pair iteration walks the upper triangle of
/// every counter row (`j > i`, nonzero); the set of `(j, co_changes)` pairs is
/// independent of map iteration order, so the result is deterministic.
#[must_use]
pub fn compute_aggregate(input: &ReportData) -> AggregateData {
    let mut agg = AggregateData {
        total_nodes: input.nodes.len() as i64,
        ..AggregateData::default()
    };

    let mut total_changes: i64 = 0;
    let mut total_couplings: i64 = 0;
    let mut hot_nodes: i64 = 0;
    let mut strength_sum: f64 = 0.0;
    let mut pair_count: i64 = 0;

    for (i, counters) in input.counters.iter().enumerate() {
        let self_i = counter_at(counters, i);
        total_changes += self_i;

        if self_i >= HOTSPOT_THRESHOLD_MEDIUM {
            hot_nodes += 1;
        }

        let mut cols: Vec<usize> = counters
            .keys()
            .copied()
            .filter(|&j| j > i && counter_at(counters, j) != 0)
            .collect();
        cols.sort_unstable();

        for j in cols {
            let co_changes = counter_at(counters, j);

            total_couplings += 1;
            pair_count += 1;

            let self_j = if j < input.counters.len() {
                counter_at(&input.counters[j], j)
            } else {
                0
            };

            strength_sum += compute_coupling_strength(co_changes, self_i, self_j);
        }
    }

    agg.total_changes = total_changes;
    agg.total_couplings = total_couplings;
    agg.hot_nodes = hot_nodes;

    if agg.total_nodes > 0 {
        agg.avg_changes_per_node = total_changes as f64 / agg.total_nodes as f64;
    }

    if pair_count > 0 {
        agg.avg_coupling_strength = strength_sum / pair_count as f64;
    }

    agg
}

/// Runs all shotness metrics over the parsed report. Port of `ComputeAllMetrics`.
#[must_use]
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    ComputedMetrics {
        node_hotness: compute_node_hotness(input),
        node_coupling: compute_node_coupling(input),
        hotspot_nodes: compute_hotspot_nodes(input),
        aggregate: compute_aggregate(input),
    }
}

/// Stable descending sort by an integer key.
///
/// Go's `sort.Slice` with a strict `a > b` comparator is unstable in general,
/// but the shotness comparators only compare the count, and the upstream input
/// order (report node order / matrix order) is deterministic. A stable sort
/// keeps equal-count rows in that deterministic order, which the byte-identity
/// goldens require.
fn sort_by_desc_stable<T, F: Fn(&T) -> i64>(v: &mut [T], key: F) {
    v.sort_by(|a, b| key(b).cmp(&key(a)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hm(pairs: &[(usize, i64)]) -> HashMap<usize, i64> {
        pairs.iter().copied().collect()
    }

    // Ported from TestNodeHotnessMetric_Empty.
    #[test]
    fn compute_node_hotness_empty() {
        assert_eq!(compute_node_hotness(&ReportData::default()), vec![]);
    }

    // Ported from TestNodeHotnessMetric_ValidData.
    #[test]
    fn compute_node_hotness_valid_data() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("function", "TestFunc1", "file1.go"),
                NodeSummary::new("function", "TestFunc2", "file1.go"),
            ],
            counters: vec![hm(&[(0, 10), (1, 5)]), hm(&[(0, 5), (1, 20)])],
        };
        let result = compute_node_hotness(&input);
        assert_eq!(result.len(), 2);
        // Node 2 first (20 > 10).
        assert_eq!(result[0].name, "TestFunc2");
        assert_eq!(result[0].change_count, 20);
        assert_eq!(result[0].coupled_nodes, 1);
        assert!((result[0].hotness_score - 1.0).abs() < 0.01);
        assert_eq!(result[1].name, "TestFunc1");
        assert_eq!(result[1].change_count, 10);
        assert!((result[1].hotness_score - 0.5).abs() < 0.01);
    }

    // Ported from TestNodeHotnessMetric_OutOfBounds.
    #[test]
    fn compute_node_hotness_out_of_bounds() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("", "TestFunc1", ""),
                NodeSummary::new("", "TestFunc2", ""),
            ],
            counters: vec![hm(&[(0, 10)])],
        };
        let result = compute_node_hotness(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "TestFunc1");
    }

    // Ported from TestNodeCouplingMetric_Empty.
    #[test]
    fn compute_node_coupling_empty() {
        assert_eq!(compute_node_coupling(&ReportData::default()), vec![]);
    }

    // Ported from TestNodeCouplingMetric_ValidData.
    #[test]
    fn compute_node_coupling_valid_data() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("function", "TestFunc1", "file1.go"),
                NodeSummary::new("function", "TestFunc2", "file2.go"),
                NodeSummary::new("function", "TestFunc3", "file2.go"),
            ],
            counters: vec![
                hm(&[(0, 10), (1, 5), (2, 2)]),
                hm(&[(0, 5), (1, 20), (2, 8)]),
                hm(&[(0, 2), (1, 8), (2, 5)]),
            ],
        };
        let result = compute_node_coupling(&input);
        assert_eq!(result.len(), 3);
        // Sorted by co-changes desc: (2,3)=8, (1,2)=5, (1,3)=2.
        assert_eq!(result[0].node1_name, "TestFunc2");
        assert_eq!(result[0].node2_name, "TestFunc3");
        assert_eq!(result[0].co_changes, 8);
        assert_eq!(result[1].node1_name, "TestFunc1");
        assert_eq!(result[1].node2_name, "TestFunc2");
        assert_eq!(result[1].co_changes, 5);
        assert_eq!(result[2].node1_name, "TestFunc1");
        assert_eq!(result[2].node2_name, "TestFunc3");
        assert_eq!(result[2].co_changes, 2);
    }

    // Ported from TestNodeCouplingMetric_ZeroCoupling.
    #[test]
    fn compute_node_coupling_zero_omitted() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("", "TestFunc1", ""),
                NodeSummary::new("", "TestFunc2", ""),
            ],
            counters: vec![hm(&[(0, 10), (1, 0)]), hm(&[(0, 0), (1, 20)])],
        };
        assert_eq!(compute_node_coupling(&input), vec![]);
    }

    // Ported from TestNodeCouplingMetric_IncludesStrength.
    #[test]
    fn compute_node_coupling_includes_strength() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("function", "TestFunc1", "file1.go"),
                NodeSummary::new("function", "TestFunc2", "file2.go"),
            ],
            counters: vec![hm(&[(0, 10), (1, 5)]), hm(&[(0, 5), (1, 20)])],
        };
        let result = compute_node_coupling(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].co_changes, 5);
        assert!((result[0].strength - 0.25).abs() < 0.01);
    }

    // Ported from TestComputeCouplingStrength_Basic.
    #[test]
    fn compute_coupling_strength_cases() {
        assert!((compute_coupling_strength(5, 5, 5) - 1.0).abs() < 0.01);
        assert!((compute_coupling_strength(5, 10, 10) - 0.5).abs() < 0.01);
        assert!((compute_coupling_strength(3, 3, 10) - 0.3).abs() < 0.01);
        assert!((compute_coupling_strength(0, 0, 0) - 0.0).abs() < 0.01);
        assert!((compute_coupling_strength(5, 3, 4) - 1.0).abs() < 0.01);
    }

    // Ported from TestHotspotNodeMetric_Empty.
    #[test]
    fn compute_hotspot_nodes_empty() {
        assert_eq!(compute_hotspot_nodes(&ReportData::default()), vec![]);
    }

    // Ported from TestHotspotNodeMetric_ValidData.
    #[test]
    fn compute_hotspot_nodes_valid_data() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("", "TestFunc1", ""),
                NodeSummary::new("", "TestFunc2", ""),
                NodeSummary::new("", "TestFunc3", ""),
            ],
            counters: vec![
                hm(&[(0, HOTSPOT_THRESHOLD_MEDIUM - 1)]),
                hm(&[(0, 0), (1, HOTSPOT_THRESHOLD_HIGH)]),
                hm(&[(0, 0), (1, 0), (2, HOTSPOT_THRESHOLD_MEDIUM)]),
            ],
        };
        let result = compute_hotspot_nodes(&input);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "TestFunc2");
        assert_eq!(result[0].risk_level, RISK_LEVEL_HIGH);
        assert_eq!(result[0].change_count, HOTSPOT_THRESHOLD_HIGH);
        assert_eq!(result[1].name, "TestFunc3");
        assert_eq!(result[1].risk_level, RISK_LEVEL_MEDIUM);
        assert_eq!(result[1].change_count, HOTSPOT_THRESHOLD_MEDIUM);
    }

    // Ported from TestClassifyChangeRisk.
    #[test]
    fn classify_change_risk_thresholds() {
        assert_eq!(classify_change_risk(HOTSPOT_THRESHOLD_MEDIUM - 1), RISK_LEVEL_LOW);
        assert_eq!(classify_change_risk(HOTSPOT_THRESHOLD_MEDIUM), RISK_LEVEL_MEDIUM);
        assert_eq!(classify_change_risk(HOTSPOT_THRESHOLD_HIGH - 1), RISK_LEVEL_MEDIUM);
        assert_eq!(classify_change_risk(HOTSPOT_THRESHOLD_HIGH), RISK_LEVEL_HIGH);
        assert_eq!(classify_change_risk(HOTSPOT_THRESHOLD_HIGH + 100), RISK_LEVEL_HIGH);
    }

    // Ported from TestAggregateMetric_Empty.
    #[test]
    fn compute_aggregate_empty() {
        assert_eq!(compute_aggregate(&ReportData::default()), AggregateData::default());
    }

    // Ported from TestAggregateMetric_ValidData.
    #[test]
    fn compute_aggregate_valid_data() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("", "TestFunc1", ""),
                NodeSummary::new("", "TestFunc2", ""),
            ],
            counters: vec![
                hm(&[(0, HOTSPOT_THRESHOLD_HIGH), (1, 5)]),
                hm(&[(0, 5), (1, 10)]),
            ],
        };
        let result = compute_aggregate(&input);
        assert_eq!(result.total_nodes, 2);
        assert_eq!(result.total_changes, HOTSPOT_THRESHOLD_HIGH + 10);
        assert_eq!(result.total_couplings, 1);
        assert!((result.avg_changes_per_node - 15.0).abs() < 0.01);
        assert_eq!(result.hot_nodes, 2);
        assert!((result.avg_coupling_strength - 0.25).abs() < 0.01);
    }

    // Ported from TestAggregateMetric_IncludesAvgCouplingStrength.
    #[test]
    fn compute_aggregate_avg_coupling_strength() {
        let input = ReportData {
            nodes: vec![
                NodeSummary::new("", "TestFunc1", ""),
                NodeSummary::new("", "TestFunc2", ""),
            ],
            counters: vec![hm(&[(0, 10), (1, 5)]), hm(&[(0, 5), (1, 10)])],
        };
        let result = compute_aggregate(&input);
        assert!((result.avg_coupling_strength - 0.5).abs() < 0.01);
    }
}

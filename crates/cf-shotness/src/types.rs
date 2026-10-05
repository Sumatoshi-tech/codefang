//! Core data types shared across the shotness analyzer.
//!
//! These are the in-memory intermediate representations; the machine-format
//! serialization lives in [`crate::report`].

use std::collections::HashMap;

/// Identifying information for a code node (UAST entity).
///
/// The [`NodeSummary::key`] derivation `Type + "_" + Name + "_" + File` is the
/// canonical entity key of the live state; `name` is the qualified name the
/// walk derives (`Class.method`, `Receiver.method`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct NodeSummary {
    /// UAST node type (e.g. the grammar type string).
    pub type_: String,
    /// The qualified entity name (enclosing declarations, then the name DSL
    /// result).
    pub name: String,
    /// Source file path the node belongs to.
    pub file: String,
}

impl NodeSummary {
    /// Constructs a [`NodeSummary`].
    pub fn new(type_: impl Into<String>, name: impl Into<String>, file: impl Into<String>) -> Self {
        Self {
            type_: type_.into(),
            name: name.into(),
            file: file.into(),
        }
    }

    /// Canonical node key: `Type + "_" + Name + "_" + File`.
    ///
    /// ```
    /// use cf_shotness::types::NodeSummary;
    ///
    /// let ns = NodeSummary::new("Function", "foo", "a.go");
    /// assert_eq!(ns.key(), "Function_foo_a.go");
    /// // Empty fields still produce the two separators.
    /// assert_eq!(NodeSummary::new("", "", "").key(), "__");
    /// ```
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}_{}_{}", self.type_, self.name, self.file)
    }
}

/// The report input of the metric computation stage, built by
/// [`crate::state::ShotnessState::report`].
///
/// `counters[i]` is the co-change row for node `i`; `counters[i][i]` is node
/// `i`'s self-change count, and `counters[i][j]` (`j != i`) is the co-change
/// count between nodes `i` and `j`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportData {
    /// Ordered list of nodes (sorted by node key).
    pub nodes: Vec<NodeSummary>,
    /// Per-node co-change counter rows, parallel to [`ReportData::nodes`].
    pub counters: Vec<HashMap<usize, i64>>,
    /// Per-node ended flag, parallel to [`ReportData::nodes`] (a missing
    /// entry means live).
    pub ended: Vec<bool>,
    /// Ended entities were kept and marked (`--shotness-keep-ended`); when
    /// false they were dropped and the report carries no ended fields.
    pub ended_tracked: bool,
}

impl ReportData {
    /// The live entities only, the counters re-pointed at their new indices
    /// (a view of the code that exists at the newest commit).
    #[must_use]
    pub fn live(&self) -> Self {
        let is_live = |i: usize| !self.ended.get(i).copied().unwrap_or(false);
        let mut new_index = HashMap::new();
        for (old, _) in self.nodes.iter().enumerate().filter(|(i, _)| is_live(*i)) {
            new_index.insert(old, new_index.len());
        }
        let mut live = Self {
            ended_tracked: self.ended_tracked,
            ..Self::default()
        };
        for (old, node) in self.nodes.iter().enumerate() {
            if !new_index.contains_key(&old) {
                continue;
            }
            live.nodes.push(node.clone());
            live.ended.push(false);
            live.counters.push(
                self.counters
                    .get(old)
                    .into_iter()
                    .flatten()
                    .filter_map(|(j, count)| Some((*new_index.get(j)?, *count)))
                    .collect(),
            );
        }
        live
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The live view drops ended nodes and re-points the counters at the
    /// remaining nodes' new indices.
    #[test]
    fn the_live_view_drops_ended_nodes_and_reindexes_counters() {
        let n = |name: &str| NodeSummary::new("Function", name, "a.go");
        let row = |pairs: &[(usize, i64)]| pairs.iter().copied().collect::<HashMap<usize, i64>>();
        let report = ReportData {
            nodes: vec![n("a"), n("gone"), n("c")],
            counters: vec![
                row(&[(0, 3), (1, 2), (2, 1)]),
                row(&[(1, 5), (0, 2)]),
                row(&[(2, 4), (0, 1)]),
            ],
            ended: vec![false, true, false],
            ended_tracked: true,
        };
        let live = report.live();
        assert_eq!(live.nodes, vec![n("a"), n("c")]);
        assert_eq!(
            live.counters,
            vec![row(&[(0, 3), (1, 1)]), row(&[(1, 4), (0, 1)])]
        );
        assert_eq!(live.ended, vec![false, false]);
    }

    #[test]
    fn node_summary_key_matches_contract_format() {
        let ns = NodeSummary::new("Function", "foo", "a.go");
        assert_eq!(ns.key(), "Function_foo_a.go");
    }

    #[test]
    fn node_summary_key_with_empty_fields() {
        let ns = NodeSummary::new("", "", "");
        assert_eq!(ns.key(), "__");
    }
}

//! Core data types shared across the shotness analyzer.
//!
//! Mirrors the type declarations in `internal/analyzers/shotness/analyzer.go`
//! and `metrics.go`. These are the in-memory intermediate representations; the
//! machine-format serialization lives in [`crate::report`].

use std::collections::HashMap;

/// Identifying information for a code node (UAST entity).
///
/// Mirrors Go `shotness.NodeSummary`. The [`NodeSummary::key`] derivation
/// `Type + "_" + Name + "_" + File` is the canonical node key used for last-wins
/// collision resolution and additive merge across commits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeSummary {
    /// UAST node type (e.g. the grammar type string).
    pub type_: String,
    /// Extracted display name (from the name DSL, or the node token).
    pub name: String,
    /// Source file path the node belongs to.
    pub file: String,
}

impl NodeSummary {
    /// Construct a [`NodeSummary`].
    pub fn new(type_: impl Into<String>, name: impl Into<String>, file: impl Into<String>) -> Self {
        NodeSummary {
            type_: type_.into(),
            name: name.into(),
            file: file.into(),
        }
    }

    /// Canonical node key: `Type + "_" + Name + "_" + File`.
    ///
    /// Mirrors Go `(*NodeSummary).String()`.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}_{}_{}", self.type_, self.name, self.file)
    }
}

/// Parsed report input for the metric computation stage.
///
/// Mirrors Go `shotness.ReportData`. `counters[i]` is the co-change row for
/// node `i`; `counters[i][i]` is node `i`'s self-change count, and
/// `counters[i][j]` (`j != i`) is the co-change count between nodes `i` and `j`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReportData {
    /// Ordered list of nodes (sorted by node key when produced by the report
    /// builder).
    pub nodes: Vec<NodeSummary>,
    /// Per-node co-change counter rows, parallel to [`ReportData::nodes`].
    pub counters: Vec<HashMap<usize, i64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_summary_key_matches_go_format() {
        let ns = NodeSummary::new("Function", "foo", "a.go");
        assert_eq!(ns.key(), "Function_foo_a.go");
    }

    #[test]
    fn node_summary_key_with_empty_fields() {
        let ns = NodeSummary::new("", "", "");
        assert_eq!(ns.key(), "__");
    }
}

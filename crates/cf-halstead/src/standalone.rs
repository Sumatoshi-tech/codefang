//! Standalone per-file Halstead analysis.
//!
//! This is the path used by the **quality** history analyzer (NOT the
//! streaming visitor path the static pipeline uses). For each changed file's
//! UAST root it:
//!
//!  1. finds every function node — UAST `Function`/`Method` type ∪ `Function`
//!     role, at traversal depth ≤ [`crate::MAX_DEPTH`], each node once;
//!  2. collects operators/operands over each function's **full subtree**;
//!  3. aggregates per-function operator/operand maps into file-level distinct/
//!     total counts and derives the file-level measures.
//!
//! Only the file-level scalars the quality analyzer reads — `volume`,
//! `effort`, `delivered_bugs` — are returned; the CMS `estimated_total_*` path
//! is omitted because it never feeds those scalars. A file with no functions
//! yields all zeros.

use std::collections::HashMap;

use cf_uast_node::Node;

use crate::calculator::{HalsteadCounts, MetricsCalculator};
use crate::detector::{HalNode, OperatorOperandDetector};

/// File-level Halstead measures consumed by the quality analyzer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FileHalstead {
    /// V — file-level volume.
    pub volume: f64,
    /// E — file-level effort.
    pub effort: f64,
    /// B — file-level delivered bugs.
    pub delivered_bugs: f64,
}

/// [`HalNode`] adapter over the production `cf_uast_node::Node`.
///
/// The detector reads the type, token, roles, props, and children; these map
/// directly onto the node fields. The role/type strings are the canonical
/// UAST strings the parser emits, which is what the detector matches.
impl HalNode for Node {
    fn node_type(&self) -> &str {
        &self.node_type
    }
    fn token(&self) -> &str {
        &self.token
    }
    fn has_any_role(&self, roles: &[&str]) -> bool {
        Node::has_any_role(self, roles)
    }
    fn prop(&self, key: &str) -> Option<&str> {
        self.props.get(key).map(String::as_str)
    }
    fn children(&self) -> &[Self] {
        &self.children
    }
}

/// Runs the standalone Halstead analysis over `root`, returning the file-level
/// measures.
#[must_use]
#[allow(clippy::cast_possible_wrap)] // distinct-token counts fit i64
pub fn analyze(root: &Node) -> FileHalstead {
    let functions = find_functions(root);
    if functions.is_empty() {
        return FileHalstead::default();
    }

    let detector = OperatorOperandDetector::new();
    let calc = MetricsCalculator::new();

    // Aggregate per-function operator/operand maps into file-level maps.
    let mut file_operators: HashMap<String, i64> = HashMap::new();
    let mut file_operands: HashMap<String, i64> = HashMap::new();

    for f_node in &functions {
        let mut operators: HashMap<String, i64> = HashMap::new();
        let mut operands: HashMap<String, i64> = HashMap::new();
        detector.collect(*f_node, &mut operators, &mut operands);

        for (op, count) in &operators {
            *file_operators.entry(op.clone()).or_insert(0) += *count;
        }
        for (opnd, count) in &operands {
            *file_operands.entry(opnd.clone()).or_insert(0) += *count;
        }
    }

    let counts = HalsteadCounts {
        distinct_operators: file_operators.len() as i64,
        distinct_operands: file_operands.len() as i64,
        total_operators: calc.sum_map(&file_operators),
        total_operands: calc.sum_map(&file_operands),
    };
    let d = calc.calculate(counts);

    FileHalstead {
        volume: d.volume,
        effort: d.effort,
        delivered_bugs: d.delivered_bugs,
    }
}

/// Finds all function nodes: UAST `Function`/`Method` types ∪ `Function` role,
/// traversal depth ≤ [`crate::MAX_DEPTH`], each node once.
///
/// The reference behavior unions a type-traversal and a role-traversal into an
/// identity set; both share the same pre-order DFS, so a node matching both
/// appears once. A single DFS that yields any node matching either criterion
/// is equivalent because each node is visited exactly once.
fn find_functions(root: &Node) -> Vec<&Node> {
    let mut out: Vec<&Node> = Vec::new();
    // Iterative pre-order DFS with depth; root at depth 0.
    let mut stack: Vec<(&Node, i64)> = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        if depth <= crate::MAX_DEPTH {
            let by_type = node.node_type == "Function" || node.node_type == "Method";
            let by_role = HalNode::has_any_role(node, &["Function"]);
            if by_type || by_role {
                out.push(node);
            }
        }
        for child in node.children.iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree_is_zero() {
        let root = Node {
            node_type: "File".to_string(),
            ..Node::default()
        };
        let r = analyze(&root);
        assert_eq!(r, FileHalstead::default());
    }
}

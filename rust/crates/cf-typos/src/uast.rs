//! Self-contained UAST node type and pre-order walker.
//!
//! Mirrors the slice of `cf_uast_node::Node` (Go `pkg/uast/pkg/node`) the typos
//! analyzer needs: `node_type`, `token`, `children`, and `pos.start_line`, plus
//! a pre-order visit. The `cf-uast-node` crate is not yet implemented in this
//! workspace; replacing this module with a dependency on it is mechanical (the
//! field names mirror that crate). The identifier type constant matches Go
//! `node.UASTIdentifier`.

/// UAST node type constant for identifiers (Go `node.UASTIdentifier`).
pub const UAST_IDENTIFIER: &str = "Identifier";

/// Positional information for a node (subset of Go `node.Positions`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Positions {
    /// 1-based start line.
    pub start_line: u32,
}

/// A Universal Abstract Syntax Tree node (subset used by the typos analyzer).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    /// Node type, e.g. `"Identifier"`.
    pub node_type: String,
    /// Source token / literal text.
    pub token: String,
    /// Child nodes, in source order.
    pub children: Vec<Node>,
    /// Positional information (`None` if absent).
    pub pos: Option<Positions>,
}

impl Node {
    /// Creates a new node of the given type.
    pub fn new(node_type: impl Into<String>) -> Self {
        Node {
            node_type: node_type.into(),
            ..Default::default()
        }
    }

    /// Visits this node and all descendants in pre-order (self first).
    ///
    /// Port of Go `Node.VisitPreOrder`.
    pub fn visit_pre_order<F: FnMut(&Node)>(&self, f: &mut F) {
        f(self);
        for child in &self.children {
            child.visit_pre_order(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pre_order_is_self_then_children() {
        let mut root = Node::new("File");
        root.token = "root".to_string();
        let mut child = Node::new("Block");
        child.token = "child".to_string();
        child.children.push({
            let mut g = Node::new(UAST_IDENTIFIER);
            g.token = "grand".to_string();
            g
        });
        root.children.push(child);

        let mut order = Vec::new();
        root.visit_pre_order(&mut |n| order.push(n.token.clone()));
        assert_eq!(order, vec!["root", "child", "grand"]);
    }
}

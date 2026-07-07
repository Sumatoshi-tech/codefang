//! Structural equality and deterministic sorting.

use crate::node::Node;

impl Node {
    /// Compares two nodes for structural equality, *ignoring positions*:
    /// type, token, roles (order-sensitive), props (set equality), and
    /// recursively the children.
    #[must_use]
    pub fn equal(&self, other: &Self) -> bool {
        self.node_type == other.node_type
            && self.token == other.token
            && self.roles == other.roles
            && self.props == other.props
            && self.children.len() == other.children.len()
            && self
                .children
                .iter()
                .zip(&other.children)
                .all(|(a, b)| a.equal(b))
    }
}

/// Sorts nodes by type then token, for deterministic output. Used by the query
/// DSL's `sort` builtin.
pub fn sort_nodes(nodes: &mut [Node]) {
    nodes.sort_by(|a, b| {
        a.node_type
            .cmp(&b.node_type)
            .then_with(|| a.token.cmp(&b.token))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_cases() {
        assert!(Node::with_token("Function", "foo").equal(&Node::with_token("Function", "foo")));
        assert!(!Node::with_token("Function", "").equal(&Node::with_token("Method", "")));
    }

    #[test]
    fn equal_ignores_positions() {
        let mut a = Node::with_token("Function", "x");
        a.pos = Some(crate::node::Positions {
            start_line: 1,
            ..Default::default()
        });
        let b = Node::with_token("Function", "x");
        assert!(a.equal(&b));
    }

    #[test]
    fn sort_nodes_by_type_then_token() {
        let mut nodes = vec![
            Node::with_token("Function", "b"),
            Node::with_token("Function", "a"),
            Node::with_token("Class", "z"),
        ];
        sort_nodes(&mut nodes);
        assert_eq!(nodes[0].node_type, "Class");
        assert_eq!(nodes[1].token, "a");
        assert_eq!(nodes[2].token, "b");
    }
}

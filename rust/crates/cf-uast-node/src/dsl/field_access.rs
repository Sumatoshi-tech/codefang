//! Field-access strategies for the DSL.
//!
//! A field name resolves a node to a (possibly empty) set of nodes. Built-in
//! fields are `children`, `token`, `id`, `roles`, `type`, `first`, `last`; any
//! other name falls back to a property lookup (yielding a literal node holding
//! the property value).

use crate::node::Node;
use std::collections::HashMap;

/// A field access strategy: maps a node to the nodes reachable via a field.
type Strategy = fn(&Node) -> Vec<Node>;

/// Registry of field-access strategies, pre-populated with the built-in set.
pub struct FieldAccessStrategyRegistry {
    strategies: HashMap<&'static str, Strategy>,
}

impl Default for FieldAccessStrategyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldAccessStrategyRegistry {
    /// Creates a registry with the default strategies registered.
    pub fn new() -> Self {
        let mut strategies: HashMap<&'static str, Strategy> = HashMap::new();
        strategies.insert("children", children_strategy);
        strategies.insert("token", token_strategy);
        strategies.insert("id", id_strategy);
        strategies.insert("roles", roles_strategy);
        strategies.insert("type", type_strategy);
        strategies.insert("first", first_strategy);
        strategies.insert("last", last_strategy);
        Self { strategies }
    }

    /// Resolves `field_name` on `node`. Falls back to a property lookup for
    /// unknown names.
    #[must_use]
    pub fn access(&self, node: &Node, field_name: &str) -> Vec<Node> {
        if let Some(strategy) = self.strategies.get(field_name) {
            return strategy(node);
        }
        if let Some(val) = node.props.get(field_name) {
            return vec![Node::literal(val.clone())];
        }
        Vec::new()
    }
}

/// Executes field access using the default strategy registry.
pub struct FieldAccessExecutor {
    registry: FieldAccessStrategyRegistry,
}

impl Default for FieldAccessExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl FieldAccessExecutor {
    /// Creates a new executor backed by a default registry.
    #[must_use]
    pub fn new() -> Self {
        Self { registry: FieldAccessStrategyRegistry::new() }
    }

    /// Performs field access for `field_name` on `node`.
    #[must_use]
    pub fn execute(&self, node: &Node, field_name: &str) -> Vec<Node> {
        self.registry.access(node, field_name)
    }
}

// --- Strategy implementations ---

/// `children` → the node's children.
fn children_strategy(node: &Node) -> Vec<Node> {
    node.children.clone()
}

/// `token` → a literal node holding the token.
fn token_strategy(node: &Node) -> Vec<Node> {
    vec![Node::literal(node.token.clone())]
}

/// `id` → a literal node holding the ID.
///
/// The ID is raw bytes, so it is lossily decoded to a string for the literal
/// token (this path is query-only, never serialized, and IDs are usually empty
/// in query contexts).
fn id_strategy(node: &Node) -> Vec<Node> {
    vec![Node::literal(String::from_utf8_lossy(&node.id).into_owned())]
}

/// `roles` → one literal node per role.
fn roles_strategy(node: &Node) -> Vec<Node> {
    node.roles.iter().map(|r| Node::literal(r.clone())).collect()
}

/// `type` → a literal node holding the type, or empty if the type is empty.
fn type_strategy(node: &Node) -> Vec<Node> {
    if node.node_type.is_empty() {
        Vec::new()
    } else {
        vec![Node::literal(node.node_type.clone())]
    }
}

/// `first` → the first child, if any.
fn first_strategy(node: &Node) -> Vec<Node> {
    node.children.first().cloned().into_iter().collect()
}

/// `last` → the last child, if any.
fn last_strategy(node: &Node) -> Vec<Node> {
    node.children.last().cloned().into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Builder;

    fn tree() -> Node {
        let mut props = HashMap::new();
        props.insert("lang".to_string(), "go".to_string());
        let mut root = Builder::new()
            .with_type("Function")
            .with_token("foo")
            .with_roles(vec!["Declaration".into(), "Name".into()])
            .with_props(props)
            .build();
        root.add_child(Node::with_token("Identifier", "a"));
        root.add_child(Node::with_token("Identifier", "b"));
        root
    }

    #[test]
    fn children_field() {
        let r = FieldAccessStrategyRegistry::new();
        assert_eq!(r.access(&tree(), "children").len(), 2);
    }

    #[test]
    fn token_type_fields() {
        let r = FieldAccessStrategyRegistry::new();
        assert_eq!(r.access(&tree(), "token")[0].token, "foo");
        assert_eq!(r.access(&tree(), "type")[0].token, "Function");
    }

    #[test]
    fn roles_field_yields_one_per_role() {
        let r = FieldAccessStrategyRegistry::new();
        let roles = r.access(&tree(), "roles");
        assert_eq!(roles.len(), 2);
        assert_eq!(roles[0].token, "Declaration");
    }

    #[test]
    fn first_last_fields() {
        let r = FieldAccessStrategyRegistry::new();
        assert_eq!(r.access(&tree(), "first")[0].token, "a");
        assert_eq!(r.access(&tree(), "last")[0].token, "b");
    }

    #[test]
    fn unknown_field_falls_back_to_prop() {
        let r = FieldAccessStrategyRegistry::new();
        let out = r.access(&tree(), "lang");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].token, "go");
        assert!(r.access(&tree(), "missing").is_empty());
    }

    #[test]
    fn type_field_empty_when_no_type() {
        let r = FieldAccessStrategyRegistry::new();
        let n = Node::with_token("", "x");
        assert!(r.access(&n, "type").is_empty());
    }

    #[test]
    fn executor_delegates_to_registry() {
        let ex = FieldAccessExecutor::new();
        assert_eq!(ex.execute(&tree(), "children").len(), 2);
    }
}

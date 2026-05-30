//! Tree-sitter → UAST mapping rule and grammar metadata types.
//!
//! Direct port of Go `pkg/uast/pkg/mapping/mapping_types.go`. Field names and
//! semantics mirror the Go structs so that downstream behavior (rule extraction,
//! grammar analysis, DSL generation) is reproduced exactly.

use std::collections::BTreeMap;

/// Metadata for a Tree-sitter node type (mirrors Go `NodeTypeInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeTypeInfo {
    /// Node type name.
    pub name: String,
    /// Fields keyed by field name.
    ///
    /// A `BTreeMap` is used so the field set is deterministically ordered;
    /// the Go original uses a `map[string]FieldInfo`, whose iteration order is
    /// randomized. Every consumer in the Go code that depends on order sorts
    /// first (see [`crate::grammar_analysis::collect_child_types`]), so a sorted
    /// map reproduces the observable behavior deterministically.
    pub fields: BTreeMap<String, FieldInfo>,
    /// Child node types.
    pub children: Vec<ChildInfo>,
    /// Heuristic classification (Leaf, Container, Operator).
    pub category: NodeCategory,
    /// Whether the node is "named" in the grammar.
    pub is_named: bool,
}

/// Describes a field within a Tree-sitter node type (mirrors Go `FieldInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FieldInfo {
    /// Field name.
    pub name: String,
    /// Allowed type names for the field.
    pub types: Vec<String>,
    /// Whether the field is required.
    pub required: bool,
    /// Whether the field can hold multiple values.
    pub multiple: bool,
}

/// Describes a child node type (mirrors Go `ChildInfo`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChildInfo {
    /// Child type name.
    pub r#type: String,
    /// Whether the child is "named".
    pub named: bool,
}

/// Classifies a Tree-sitter node as Leaf, Container, or Operator.
///
/// The discriminants match the Go `iota` order (`Leaf = 0`, `Container = 1`,
/// `Operator = 2`) so the integer values are identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeCategory {
    /// A leaf node (no children, no fields).
    Leaf = 0,
    /// A container node.
    Container = 1,
    /// An operator node.
    Operator = 2,
}

impl Default for NodeCategory {
    fn default() -> Self {
        // Go zero value of NodeCategory is `Leaf` (iota 0).
        NodeCategory::Leaf
    }
}

/// A mapping from a Tree-sitter pattern to a UAST specification (Go `Rule`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rule {
    /// Rule name (the DSL identifier on the left of `<-`).
    pub name: String,
    /// The S-expression / DSL pattern (raw text, including the parentheses).
    pub pattern: String,
    /// Optional base rule name this rule extends.
    pub extends: String,
    /// Target UAST specification.
    pub uast_spec: UastSpec,
    /// Optional conditional logic.
    pub conditions: Vec<Condition>,
}

/// A conditional expression in a mapping rule (Go `Condition`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Condition {
    /// The condition expression as parsed from the DSL.
    pub expr: String,
}

/// The target UAST node structure for a mapping rule (Go `UASTSpec`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UastSpec {
    /// UAST type name.
    pub r#type: String,
    /// Token field (e.g. `@name`).
    pub token: String,
    /// Roles.
    pub roles: Vec<String>,
    /// Additional properties (key → value).
    ///
    /// `None` mirrors the Go nil map; it is lazily allocated by
    /// [`crate::dsl_parser`] only when a non-reserved field is encountered, so
    /// `Rule.uast_spec.props.is_none()` matches the Go `Props == nil` check.
    pub props: Option<BTreeMap<String, String>>,
    /// Child references.
    pub children: Vec<String>,
}

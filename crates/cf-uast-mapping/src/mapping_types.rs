//! Tree-sitter → UAST mapping rule and grammar metadata types.
//!
//! Field semantics are part of the mapping pipeline's frozen behavior (rule
//! extraction, grammar analysis, DSL generation).

use std::collections::BTreeMap;

/// Metadata for a Tree-sitter node type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodeTypeInfo {
    /// Node type name.
    pub name: String,
    /// Fields keyed by field name.
    ///
    /// A `BTreeMap` keeps the field set deterministically ordered; every
    /// consumer that depends on order sorts first (see
    /// `grammar_analysis::collect_child_types`), so a sorted map reproduces the
    /// observable behavior deterministically.
    pub fields: BTreeMap<String, FieldInfo>,
    /// Child node types.
    pub children: Vec<ChildInfo>,
    /// Heuristic classification (Leaf, Container, Operator).
    pub category: NodeCategory,
    /// Whether the node is "named" in the grammar.
    pub is_named: bool,
}

/// Describes a field within a Tree-sitter node type.
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

/// Describes a child node type.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChildInfo {
    /// Child type name.
    pub r#type: String,
    /// Whether the child is "named".
    pub named: bool,
}

/// Classifies a Tree-sitter node as Leaf, Container, or Operator.
///
/// The discriminant values (`Leaf = 0`, `Container = 1`, `Operator = 2`) are
/// frozen: they are observable wherever a category is rendered numerically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeCategory {
    /// A leaf node (no children, no fields).
    #[default]
    Leaf = 0,
    /// A container node.
    Container = 1,
    /// An operator node.
    Operator = 2,
}

/// A mapping from a Tree-sitter pattern to a UAST specification.
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

/// A conditional expression in a mapping rule.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Condition {
    /// The condition expression as parsed from the DSL.
    pub expr: String,
}

/// The target UAST node structure for a mapping rule.
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
    /// `None` means "never populated": [`crate::dsl_parser`] allocates the map
    /// lazily, only when a non-reserved field is encountered, and downstream
    /// consumers distinguish `None` from an empty map.
    pub props: Option<BTreeMap<String, String>>,
    /// Child references.
    pub children: Vec<String>,
}

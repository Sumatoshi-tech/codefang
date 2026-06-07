//! UAST node abstraction used by the cohesion analyzer.
//!
//! The Go code operates on `*node.Node` from `pkg/uast/pkg/node` and uses the
//! generic `common.UASTTraverser` / `common.DataExtractor` helpers. While the shared
//! `cf-uast-node` and `cf-analyzers-common` crates are still being ported, this
//! module defines the **minimal trait surface** the cohesion analyzer actually needs.
//!
//! In the integrated workspace [`Node`] is implemented for / replaced by
//! `cf_uast_node::Node`, and the type/role string constants come from that crate.
//! See the crate todos.
//!
//! # Constants
//!
//! The string constants below mirror the UAST type/role names referenced by the Go
//! cohesion code (`node.UASTFunction`, `node.RoleDeclaration`, …). They must match
//! `cf-uast-node` exactly because they select which AST nodes count as functions /
//! variables, which in turn determines the report bytes.

/// UAST node *type* names referenced by cohesion.
pub mod ty {
    /// `node.UASTFunction`.
    pub const FUNCTION: &str = "Function";
    /// `node.UASTMethod`.
    pub const METHOD: &str = "Method";
    /// `node.UASTVariable`.
    pub const VARIABLE: &str = "Variable";
    /// `node.UASTParameter`.
    pub const PARAMETER: &str = "Parameter";
    /// `node.UASTIdentifier`.
    pub const IDENTIFIER: &str = "Identifier";
}

/// UAST *role* names referenced by cohesion.
pub mod role {
    /// `node.RoleFunction`.
    pub const FUNCTION: &str = "Function";
    /// `node.RoleDeclaration`.
    pub const DECLARATION: &str = "Declaration";
    /// `node.RoleVariable`.
    pub const VARIABLE: &str = "Variable";
    /// `node.RoleName`.
    pub const NAME: &str = "Name";
}

/// The subset of UAST node behavior the cohesion analyzer relies on.
///
/// Implementations come from the UAST parser; tests use [`TestNode`].
pub trait Node {
    /// Child nodes, in source order (Go `node.Children`).
    fn children(&self) -> &[Self]
    where
        Self: Sized;

    /// True if the node's type is any of `types` (Go `node.HasAnyType`).
    fn has_any_type(&self, types: &[&str]) -> bool;

    /// True if the node has any of `roles` (Go `node.HasAnyRole`).
    fn has_any_role(&self, roles: &[&str]) -> bool;

    /// True if the node has *all* of `roles` (Go `node.HasAllRoles`).
    fn has_all_roles(&self, roles: &[&str]) -> bool;

    /// The entity name carried by this node, if any (Go
    /// `common.ExtractEntityName` / `extractor.ExtractName`). Empty string means
    /// "no name", matching the Go `name == ""` checks.
    ///
    /// Returns an owned `String` because the real UAST implementation derives the
    /// name from props/token/child (Go `ExtractEntityName`), which is not a borrow
    /// of any single field.
    fn entity_name(&self) -> String;

    /// Number of source lines spanned by the node (Go `traverser.CountLines`).
    fn count_lines(&self) -> i64;
}

/// A simple in-memory [`Node`] implementation for unit tests.
///
/// This mirrors enough of the UAST node shape to drive the cohesion algorithm in
/// the ported Go tests.
#[derive(Debug, Clone, Default)]
pub struct TestNode {
    /// Node type names.
    pub types: Vec<String>,
    /// Node role names.
    pub roles: Vec<String>,
    /// Entity name (empty = none).
    pub name: String,
    /// Source line span.
    pub lines: i64,
    /// Children.
    pub children: Vec<TestNode>,
}

impl TestNode {
    /// Builds a function node of the given name with the given child nodes.
    #[must_use]
    pub fn function(name: &str, lines: i64, children: Vec<TestNode>) -> Self {
        TestNode {
            types: vec![ty::FUNCTION.to_string()],
            roles: vec![role::FUNCTION.to_string()],
            name: name.to_string(),
            lines,
            children,
        }
    }

    /// Builds a variable-declaration node.
    #[must_use]
    pub fn variable(name: &str) -> Self {
        TestNode {
            types: vec![ty::VARIABLE.to_string()],
            roles: vec![role::DECLARATION.to_string()],
            name: name.to_string(),
            lines: 1,
            children: vec![],
        }
    }

    /// Builds a variable *identifier* node.
    #[must_use]
    pub fn identifier(name: &str) -> Self {
        TestNode {
            types: vec![ty::IDENTIFIER.to_string()],
            roles: vec![role::VARIABLE.to_string()],
            name: name.to_string(),
            lines: 1,
            children: vec![],
        }
    }

    /// Builds a plain container node with children.
    #[must_use]
    pub fn block(children: Vec<TestNode>) -> Self {
        TestNode {
            children,
            ..Default::default()
        }
    }
}

// === Real UAST node adapter ===
//
// Implements the cohesion [`Node`] surface for the shared `cf_uast_node::Node`, so
// the static pipeline can drive the analyzer over parsed source. Each method maps
// to its Go counterpart in `pkg/uast/pkg/node` + `internal/analyzers/common`.

impl Node for cf_uast_node::Node {
    fn children(&self) -> &[Self] {
        &self.children
    }

    fn has_any_type(&self, types: &[&str]) -> bool {
        cf_uast_node::Node::has_any_type(self, types)
    }

    fn has_any_role(&self, roles: &[&str]) -> bool {
        cf_uast_node::Node::has_any_role(self, roles)
    }

    fn has_all_roles(&self, roles: &[&str]) -> bool {
        cf_uast_node::Node::has_all_roles(self, roles)
    }

    fn entity_name(&self) -> String {
        extract_entity_name(self).unwrap_or_default()
    }

    fn count_lines(&self) -> i64 {
        // Go `UASTTraverser.CountLines`: (end_line - start_line + 1) for this node
        // when a position is present, plus the recursive sum over children.
        let mut total: i64 = 0;
        if let Some(pos) = &self.pos {
            total = pos.end_line as i64 - pos.start_line as i64 + 1;
        }
        for child in &self.children {
            total += Node::count_lines(child);
        }
        total
    }
}

/// Go `common.ExtractEntityName`: try `props["name"]`, then a non-empty token,
/// then the first child's token / `props["name"]`.
fn extract_entity_name(n: &cf_uast_node::Node) -> Option<String> {
    if let Some(name) = n.props.get("name") {
        return Some(name.clone());
    }
    if !n.token.is_empty() {
        return Some(n.token.clone());
    }
    let child = n.children.first()?;
    if !child.token.is_empty() {
        return Some(child.token.clone());
    }
    child.props.get("name").cloned()
}

impl Node for TestNode {
    fn children(&self) -> &[Self] {
        &self.children
    }

    fn has_any_type(&self, types: &[&str]) -> bool {
        types.iter().any(|t| self.types.iter().any(|s| s == t))
    }

    fn has_any_role(&self, roles: &[&str]) -> bool {
        roles.iter().any(|r| self.roles.iter().any(|s| s == r))
    }

    fn has_all_roles(&self, roles: &[&str]) -> bool {
        roles.iter().all(|r| self.roles.iter().any(|s| s == r))
    }

    fn entity_name(&self) -> String {
        self.name.clone()
    }

    fn count_lines(&self) -> i64 {
        self.lines
    }
}

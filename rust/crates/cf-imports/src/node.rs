//! Minimal UAST node model (local shim).
//!
//! Mirrors the subset of `pkg/uast/pkg/node.Node` that the Go
//! `internal/analyzers/imports` package reads: `Type`, `Token`, `Roles`,
//! `Children`. Once the real `cf-uast-node` crate is ported, delete this module
//! and use `cf_uast_node::Node` (full `ToMap`/JSON parity); the import analyzer
//! only touches the fields modelled here.
//!
//! The Go type tags used by the analyzer are reproduced as associated
//! constants on [`uast`] and [`role`] so the ported traversal reads exactly
//! like the Go code (`n.Type == node.UASTImport`, `n.HasAnyRole(node.RoleImport)`,
//! `child.Type == node.UASTLiteral`, `child.Type == node.UASTIdentifier`).

/// A semantic role tag attached to a node (Go `node.Role`, a string type).
pub type Role = String;

/// Canonical UAST node-type strings used by the imports analyzer.
///
/// These match the Go `node` package constants (`UASTImport`, `UASTLiteral`,
/// `UASTIdentifier`).
pub mod uast {
    /// `node.UASTImport` — an import statement node.
    pub const IMPORT: &str = "Import";
    /// `node.UASTLiteral` — a literal (e.g. a quoted import path).
    pub const LITERAL: &str = "Literal";
    /// `node.UASTIdentifier` — an identifier (e.g. a module name).
    pub const IDENTIFIER: &str = "Identifier";
}

/// Canonical UAST role strings used by the imports analyzer.
pub mod role {
    /// `node.RoleImport` — marks a node as participating in an import.
    pub const IMPORT: &str = "Import";
}

/// A node in the Universal Abstract Syntax Tree.
///
/// Field-for-field mirror (restricted to the fields the imports analyzer uses)
/// of the Go `node.Node`. `children` holds owned child nodes; the Go code uses
/// `[]*node.Node`, but ownership is irrelevant to the read-only traversal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Node {
    /// The node type (e.g. [`uast::IMPORT`]).
    pub type_: String,
    /// The raw token text, if any.
    pub token: String,
    /// Semantic roles attached to this node.
    pub roles: Vec<Role>,
    /// Child nodes.
    pub children: Vec<Node>,
}

impl Node {
    /// Creates a node with the given type and no other attributes.
    pub fn new(type_: impl Into<String>) -> Self {
        Node {
            type_: type_.into(),
            ..Default::default()
        }
    }

    /// Builder: set the token text.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = token.into();
        self
    }

    /// Builder: set the roles.
    pub fn with_roles<I, S>(mut self, roles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.roles = roles.into_iter().map(Into::into).collect();
        self
    }

    /// Builder: append children.
    pub fn with_children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = Node>,
    {
        self.children.extend(children);
        self
    }

    /// Reports whether this node carries any of the given role.
    ///
    /// Mirrors Go `(*Node).HasAnyRole`. Here it takes a single role because the
    /// analyzer only ever queries [`role::IMPORT`].
    pub fn has_any_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Visits this node and all descendants in pre-order (node before children).
    ///
    /// Mirrors Go `(*Node).VisitPreOrder`.
    pub fn visit_pre_order<F: FnMut(&Node)>(&self, f: &mut F) {
        f(self);
        for child in &self.children {
            child.visit_pre_order(f);
        }
    }
}

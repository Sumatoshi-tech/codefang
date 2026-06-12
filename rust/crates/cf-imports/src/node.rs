//! Minimal UAST node model (local shim).
//!
//! The subset of the canonical UAST node that the import analyzer reads:
//! `type_`, `token`, `roles`, `children`. The full model (with map/JSON
//! conversion) lives in `cf-uast-node`; the analyzer only touches the fields
//! modelled here, so the crate stays self-contained.
//!
//! The node-type and role tags used by the analyzer are defined as constants on
//! [`uast`] and [`role`] so the traversal reads declaratively
//! (`n.type_ == uast::IMPORT`, `n.has_any_role(role::IMPORT)`).

/// A semantic role tag attached to a node.
pub type Role = String;

/// Canonical UAST node-type strings used by the imports analyzer.
pub mod uast {
    /// An import statement node.
    pub const IMPORT: &str = "Import";
    /// A literal (e.g. a quoted import path).
    pub const LITERAL: &str = "Literal";
    /// An identifier (e.g. a module name).
    pub const IDENTIFIER: &str = "Identifier";
}

/// Canonical UAST role strings used by the imports analyzer.
pub mod role {
    /// Marks a node as participating in an import.
    pub const IMPORT: &str = "Import";
}

/// A node in the Universal Abstract Syntax Tree (restricted to the fields the
/// imports analyzer uses).
///
/// `children` holds owned child nodes; ownership is irrelevant to the
/// read-only traversal.
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
    #[must_use]
    pub fn new(type_: impl Into<String>) -> Self {
        Node {
            type_: type_.into(),
            ..Default::default()
        }
    }

    /// Builder: set the token text.
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = token.into();
        self
    }

    /// Builder: set the roles.
    #[must_use]
    pub fn with_roles<I, S>(mut self, roles: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.roles = roles.into_iter().map(Into::into).collect();
        self
    }

    /// Builder: append children.
    #[must_use]
    pub fn with_children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = Node>,
    {
        self.children.extend(children);
        self
    }

    /// Reports whether this node carries the given role.
    ///
    /// Takes a single role because the analyzer only ever queries
    /// [`role::IMPORT`].
    #[must_use]
    pub fn has_any_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Visits this node and all descendants in pre-order (node before children).
    pub fn visit_pre_order<F: FnMut(&Node)>(&self, f: &mut F) {
        f(self);
        for child in &self.children {
            child.visit_pre_order(f);
        }
    }
}

//! UAST type and role label constants used by clone detection.
//!
//! These mirror the Go `pkg/uast/pkg/node` constants
//! (`node.UASTFunction`, `node.UASTMethod`, `node.RoleFunction`,
//! `node.RoleDeclaration`, `node.RoleParameter`). The canonical home for these
//! is the [`cf_uast_node`] crate; per DESIGN rule (5) they are mirrored locally
//! because that crate currently exposes the same names through two ambiguous
//! glob re-exports (`node::roles::*` and `types::*`), which makes the root-level
//! paths unresolvable. The string *values* are the contract that matters for
//! byte-identity (they are the labels stored on each [`cf_uast_node::Node`]), and
//! they are identical to the Go constants.

/// Function node type. Mirrors Go `node.UASTFunction`.
pub const UAST_FUNCTION: &str = "Function";
/// Method node type. Mirrors Go `node.UASTMethod`.
pub const UAST_METHOD: &str = "Method";

/// Function role. Mirrors Go `node.RoleFunction`.
pub const ROLE_FUNCTION: &str = "Function";
/// Declaration role. Mirrors Go `node.RoleDeclaration`.
pub const ROLE_DECLARATION: &str = "Declaration";
/// Parameter role. Mirrors Go `node.RoleParameter`.
pub const ROLE_PARAMETER: &str = "Parameter";
/// Name role (used by entity-name extraction). Mirrors Go `node.RoleName`.
pub const ROLE_NAME: &str = "Name";

/// Test-only fluent [`cf_uast_node::Node`] builder.
///
/// The canonical [`cf_uast_node::Builder`] is currently unresolvable at the
/// crate root because of the same ambiguous-glob issue noted above, so the test
/// suites build nodes through this minimal local helper instead.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct NodeBuilder {
    node: cf_uast_node::Node,
}

#[cfg(test)]
impl NodeBuilder {
    /// Starts a node of the given type.
    #[must_use]
    pub fn new(node_type: &str) -> Self {
        Self {
            node: cf_uast_node::Node::new(node_type),
        }
    }

    /// Sets the node token.
    #[must_use]
    pub fn token(mut self, token: &str) -> Self {
        self.node.token = token.to_string();
        self
    }

    /// Adds a role.
    #[must_use]
    pub fn role(mut self, role: &str) -> Self {
        self.node.roles.push(role.to_string());
        self
    }

    /// Adds a child node.
    #[must_use]
    pub fn child(mut self, child: cf_uast_node::Node) -> Self {
        self.node.add_child(child);
        self
    }

    /// Finalizes the node.
    #[must_use]
    pub fn build(self) -> cf_uast_node::Node {
        self.node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_values_match_go() {
        assert_eq!(UAST_FUNCTION, "Function");
        assert_eq!(UAST_METHOD, "Method");
        assert_eq!(ROLE_FUNCTION, "Function");
        assert_eq!(ROLE_DECLARATION, "Declaration");
        assert_eq!(ROLE_PARAMETER, "Parameter");
        assert_eq!(ROLE_NAME, "Name");
    }
}

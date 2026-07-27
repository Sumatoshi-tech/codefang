//! Minimal UAST node model used by the complexity analyzer.
//!
//! This covers exactly the subset of the UAST node surface the analyzer
//! reads: node type, token, roles, string props, children, and source
//! positions, plus the traversal helpers (pre-order visit, type/role
//! predicates, type/role collection) the analyzer calls.

use std::collections::BTreeMap;

/// UAST node type string constants. The byte values flow into reports via
/// node types, so they are part of the report contract.
pub mod uast {
    /// `File` root node type.
    pub const FILE: &str = "File";
    /// `Function` node type.
    pub const FUNCTION: &str = "Function";
    /// `FunctionDecl` node type.
    pub const FUNCTION_DECL: &str = "FunctionDecl";
    /// `Method` node type.
    pub const METHOD: &str = "Method";
    /// `Class` node type.
    pub const CLASS: &str = "Class";
    /// `Block` node type.
    pub const BLOCK: &str = "Block";
    /// `If` node type (decision point / nesting).
    pub const IF: &str = "If";
    /// `Loop` node type (decision point / nesting).
    pub const LOOP: &str = "Loop";
    /// `Switch` node type (nesting; cognitive increment like a loop).
    pub const SWITCH: &str = "Switch";
    /// `Case` node type (decision point unless `default`).
    pub const CASE: &str = "Case";
    /// `Try` node type (nesting).
    pub const TRY: &str = "Try";
    /// `Catch` node type (decision point / nesting).
    pub const CATCH: &str = "Catch";
    /// `BinaryOp` node type (logical operators are decision points).
    pub const BINARY_OP: &str = "BinaryOp";
    /// `Return` node type.
    pub const RETURN: &str = "Return";
    /// `Identifier` node type.
    pub const IDENTIFIER: &str = "Identifier";
    /// `Match` node type (cognitive: loop-like nesting increment).
    pub const MATCH: &str = "Match";
    /// `Lambda` node type (cognitive: increases nesting for its body).
    pub const LAMBDA: &str = "Lambda";
    /// `Call` node type (cognitive: +1 for a recursive call).
    pub const CALL: &str = "Call";
}

/// UAST role string constants.
pub mod role {
    /// `Function` role.
    pub const FUNCTION: &str = "Function";
    /// `Declaration` role.
    pub const DECLARATION: &str = "Declaration";
    /// `Name` role.
    pub const NAME: &str = "Name";
    /// `Condition` role.
    pub const CONDITION: &str = "Condition";
    /// `Argument` role.
    pub const ARGUMENT: &str = "Argument";
    /// `Parameter` role.
    pub const PARAMETER: &str = "Parameter";
    /// `Return` role.
    pub const RETURN: &str = "Return";
}

/// Source positions for a node.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Positions {
    /// 1-based start line.
    pub start_line: u32,
    /// 0-based start column.
    pub start_col: u32,
    /// Byte offset of the start.
    pub start_offset: u32,
    /// 1-based end line.
    pub end_line: u32,
    /// 0-based end column.
    pub end_col: u32,
    /// Byte offset of the end.
    pub end_offset: u32,
}

/// A UAST node: a typed tree node with an optional token, semantic roles,
/// string-valued properties, ordered children, and optional source positions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Node {
    /// Node type, e.g. [`uast::FUNCTION`].
    pub node_type: String,
    /// Raw source token, if any.
    pub token: String,
    /// Semantic roles, e.g. [`role::FUNCTION`].
    pub roles: Vec<String>,
    /// String-valued properties (e.g. `name`, `operator`).
    pub props: BTreeMap<String, String>,
    /// Child nodes, in source order.
    pub children: Vec<Node>,
    /// Optional source positions.
    pub pos: Option<Positions>,
}

impl Node {
    /// Creates a node of the given type with all other fields empty.
    #[must_use]
    pub fn new(node_type: impl Into<String>) -> Self {
        Node {
            node_type: node_type.into(),
            ..Node::default()
        }
    }

    /// Creates a node with a token (e.g. an [`uast::IDENTIFIER`]).
    #[must_use]
    pub fn with_token(node_type: impl Into<String>, token: impl Into<String>) -> Self {
        Node {
            node_type: node_type.into(),
            token: token.into(),
            ..Node::default()
        }
    }

    /// Builder: set children.
    #[must_use]
    pub fn with_children(mut self, children: Vec<Node>) -> Self {
        self.children = children;
        self
    }

    /// Appends a child.
    pub fn add_child(&mut self, child: Node) {
        self.children.push(child);
    }

    /// Builder: set roles.
    #[must_use]
    pub fn with_roles(mut self, roles: Vec<&str>) -> Self {
        self.roles = roles.into_iter().map(String::from).collect();
        self
    }

    /// Builder: insert one property.
    #[must_use]
    pub fn with_prop(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.props.insert(key.into(), value.into());
        self
    }

    /// Builder: set source positions.
    #[must_use]
    pub fn with_pos(mut self, pos: Positions) -> Self {
        self.pos = Some(pos);
        self
    }

    /// Looks up a string property by key.
    #[must_use]
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.get(key).map(String::as_str)
    }

    /// Reports whether this node has any of the given types.
    #[must_use]
    pub fn has_any_type(&self, types: &[&str]) -> bool {
        types.iter().any(|t| self.node_type == *t)
    }

    /// Reports whether this node has all of the given roles.
    #[must_use]
    pub fn has_all_roles(&self, roles: &[&str]) -> bool {
        roles.iter().all(|r| self.roles.iter().any(|own| own == r))
    }

    /// Reports whether this node has any of the given roles.
    #[must_use]
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        roles.iter().any(|r| self.roles.iter().any(|own| own == r))
    }

    /// Visits this node and all descendants in pre-order.
    ///
    /// Iterative (explicit stack): the trees this crate walks were built by
    /// cf-uast on a segmented stack because real sources exceed 10k nesting
    /// levels, so naive recursion here would SIGSEGV on the 8 MiB main stack.
    pub fn visit_pre_order<F: FnMut(&Node)>(&self, f: &mut F) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            f(node);
            stack.extend(node.children.iter().rev());
        }
    }

    /// Collects descendants (including `self`) whose type is in `types`, in
    /// pre-order. Iterative — see [`Self::visit_pre_order`].
    pub fn find_nodes_by_type<'a>(&'a self, types: &[&str], out: &mut Vec<&'a Node>) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.has_any_type(types) {
                out.push(node);
            }
            stack.extend(node.children.iter().rev());
        }
    }

    /// Depth-capped [`find_nodes_by_type`](Self::find_nodes_by_type): matches
    /// only nodes at depth `<= remaining` below `self` (`self` at depth 0).
    ///
    /// This mirrors the reference shared traverser's `maxDepth` behaviour: the
    /// walk's MATCHING is filtered by depth (deeper nodes are simply never
    /// matched), so pruning the descent at the cap is output-equivalent.
    pub fn find_nodes_by_type_capped<'a>(
        &'a self,
        types: &[&str],
        remaining: usize,
        out: &mut Vec<&'a Node>,
    ) {
        let mut stack = vec![(self, remaining)];
        while let Some((node, rem)) = stack.pop() {
            if node.has_any_type(types) {
                out.push(node);
            }
            if rem == 0 {
                continue;
            }
            stack.extend(node.children.iter().rev().map(|c| (c, rem - 1)));
        }
    }

    /// Collects descendants (including `self`) having any of `roles`, in
    /// pre-order.
    pub fn find_nodes_by_roles<'a>(&'a self, roles: &[&str], out: &mut Vec<&'a Node>) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            if node.has_any_role(roles) {
                out.push(node);
            }
            stack.extend(node.children.iter().rev());
        }
    }

    /// Depth-capped [`find_nodes_by_roles`](Self::find_nodes_by_roles): matches
    /// only nodes at depth `<= remaining` below `self` (`self` at depth 0).
    pub fn find_nodes_by_roles_capped<'a>(
        &'a self,
        roles: &[&str],
        remaining: usize,
        out: &mut Vec<&'a Node>,
    ) {
        let mut stack = vec![(self, remaining)];
        while let Some((node, rem)) = stack.pop() {
            if node.has_any_role(roles) {
                out.push(node);
            }
            if rem == 0 {
                continue;
            }
            stack.extend(node.children.iter().rev().map(|c| (c, rem - 1)));
        }
    }
}

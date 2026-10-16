//! Minimal UAST node model used by the complexity analyzer.
//!
//! This covers exactly the subset of the UAST node surface the analyzer
//! reads: node type, token, roles, string props, children, and source
//! positions, plus the traversal helpers the analyzer calls and the bridge
//! from the canonical [`cf_uast_node::Node`] tree ([`Node::from_uast`]).

use std::collections::BTreeMap;

/// UAST node type string constants (the subset the analyzer inspects).
pub mod uast {
    /// `File` root node type.
    pub const FILE: &str = "File";
    /// `Function` node type.
    pub const FUNCTION: &str = "Function";
    /// `Method` node type.
    pub const METHOD: &str = "Method";
    /// `Block` node type.
    pub const BLOCK: &str = "Block";
    /// `If` node type: an `if` statement or a conditional expression.
    pub const IF: &str = "If";
    /// `Loop` node type (`for` / `while` / `do` / `loop`).
    pub const LOOP: &str = "Loop";
    /// `Switch` node type.
    pub const SWITCH: &str = "Switch";
    /// `Match` node type (pattern matching; counted like a switch).
    pub const MATCH: &str = "Match";
    /// `Case` node type (a switch/match arm; a decision point unless default).
    pub const CASE: &str = "Case";
    /// `Try` node type.
    pub const TRY: &str = "Try";
    /// `Catch` node type (a decision point; a cognitive nesting node).
    pub const CATCH: &str = "Catch";
    /// `BinaryOp` node type (logical operators are decision points).
    pub const BINARY_OP: &str = "BinaryOp";
    /// `UnaryOp` node type (transparent for boolean-sequence counting).
    pub const UNARY_OP: &str = "UnaryOp";
    /// `Identifier` node type.
    pub const IDENTIFIER: &str = "Identifier";
    /// `Lambda` node type (raises cognitive nesting for its contents).
    pub const LAMBDA: &str = "Lambda";
    /// `Call` node type (`+1` cognitive for a direct recursive call).
    pub const CALL: &str = "Call";
    /// `Break` node type (`+1` cognitive when it names a label).
    pub const BREAK: &str = "Break";
    /// `Continue` node type (`+1` cognitive when it names a label).
    pub const CONTINUE: &str = "Continue";
    /// `Synthetic` node type: language-specific wrapper (parentheses, `else`
    /// clauses, statement wrappers).
    pub const SYNTHETIC: &str = "Synthetic";
    /// `Comment` node type (excluded from lines of code).
    pub const COMMENT: &str = "Comment";
    /// `Parameter` node type (signature; never part of a body).
    pub const PARAMETER: &str = "Parameter";
    /// `Field` node type (member access chain in a call target).
    pub const FIELD: &str = "Field";
    /// `Pattern` node type (match-arm pattern; `_` marks the default arm).
    pub const PATTERN: &str = "Pattern";
}

/// UAST role string constants.
pub mod role {
    /// `Function` role.
    pub const FUNCTION: &str = "Function";
    /// `Declaration` role.
    pub const DECLARATION: &str = "Declaration";
    /// `Name` role.
    pub const NAME: &str = "Name";
    /// `Type` role (a `Name`+`Type` identifier is a type reference, not a name).
    pub const TYPE: &str = "Type";
    /// `Body` role (the statement block of a function or branch).
    pub const BODY: &str = "Body";
    /// `Condition` role.
    pub const CONDITION: &str = "Condition";
    /// `Parameter` role (a parameter list wrapper is never a name source).
    pub const PARAMETER: &str = "Parameter";
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
    /// Byte offset of the end (exclusive).
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

/// Stack red zone / growth chunk for the recursive tree conversion: real
/// sources exceed 10k nesting levels (cf-uast builds them on a segmented
/// stack), so the bridge grows its stack on demand as well.
const STACK_RED_ZONE: usize = 64 * 1024;
const STACK_GROWTH: usize = 16 * 1024 * 1024;

impl Node {
    /// Creates a node of the given type with all other fields empty.
    #[must_use]
    pub fn new(node_type: impl Into<String>) -> Self {
        Node {
            node_type: node_type.into(),
            token: String::new(),
            roles: Vec::new(),
            props: BTreeMap::new(),
            children: Vec::new(),
            pos: None,
        }
    }

    /// Creates a node with a token (e.g. an [`uast::IDENTIFIER`]).
    #[must_use]
    pub fn with_token(node_type: impl Into<String>, token: impl Into<String>) -> Self {
        let mut n = Node::new(node_type);
        n.token = token.into();
        n
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

    /// Converts a parsed [`cf_uast_node::Node`] tree into this model.
    ///
    /// The one bridge every report format shares. Recursion is guarded by
    /// `stacker` because the input trees were built on a segmented stack
    /// (sources nest beyond 10k levels) and a plain recursive copy would
    /// overflow the fixed main-thread stack.
    #[must_use]
    pub fn from_uast(n: &cf_uast_node::Node) -> Self {
        stacker::maybe_grow(STACK_RED_ZONE, STACK_GROWTH, || Self::from_uast_inner(n))
    }

    fn from_uast_inner(n: &cf_uast_node::Node) -> Self {
        let mut out = Node::new(n.node_type.clone());
        out.token.clone_from(&n.token);
        out.roles.clone_from(&n.roles);
        out.props = n
            .props
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.pos = n.pos.as_ref().map(|p| Positions {
            start_line: clamp_u32(p.start_line),
            start_col: clamp_u32(p.start_col),
            start_offset: clamp_u32(p.start_offset),
            end_line: clamp_u32(p.end_line),
            end_col: clamp_u32(p.end_col),
            end_offset: clamp_u32(p.end_offset),
        });
        out.children = n.children.iter().map(Self::from_uast).collect();
        out
    }

    /// Looks up a string property by key.
    #[must_use]
    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.get(key).map(String::as_str)
    }

    /// Reports whether this node is of the given type.
    #[must_use]
    pub fn is_type(&self, node_type: &str) -> bool {
        self.node_type == node_type
    }

    /// Reports whether this node has any of the given types.
    #[must_use]
    pub fn has_any_type(&self, types: &[&str]) -> bool {
        types.iter().any(|t| self.node_type == *t)
    }

    /// Reports whether this node carries the given role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|own| own == role)
    }

    /// Reports whether this node has all of the given roles.
    #[must_use]
    pub fn has_all_roles(&self, roles: &[&str]) -> bool {
        roles.iter().all(|r| self.has_role(r))
    }

    /// Reports whether this node has any of the given roles.
    #[must_use]
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        roles.iter().any(|r| self.has_role(r))
    }

    /// The source text covered by this node's span, when both the span and
    /// the source are available and the span is well-formed.
    #[must_use]
    pub fn span_text<'a>(&self, source: Option<&'a [u8]>) -> Option<&'a str> {
        let source = source?;
        let pos = self.pos.as_ref()?;
        let (start, end) = (pos.start_offset as usize, pos.end_offset as usize);
        if start >= end || end > source.len() {
            return None;
        }
        std::str::from_utf8(&source[start..end]).ok()
    }

    /// Visits this node and all descendants in pre-order.
    ///
    /// Iterative (explicit stack): the trees this crate walks were built by
    /// cf-uast on a segmented stack because real sources exceed 10k nesting
    /// levels, so naive recursion here would overflow the main stack.
    pub fn visit_pre_order<F: FnMut(&Node)>(&self, f: &mut F) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            f(node);
            stack.extend(node.children.iter().rev());
        }
    }
}

impl Drop for Node {
    /// Iterative drop: a derived (recursive) drop of a 20k-level tree would
    /// overflow the stack exactly like the recursive walks it guards against.
    fn drop(&mut self) {
        let mut stack: Vec<Node> = std::mem::take(&mut self.children);
        while let Some(mut n) = stack.pop() {
            stack.append(&mut n.children);
        }
    }
}

/// Saturating `u64 -> u32` for source positions (a 4 GiB source file is
/// outside the supported input size; the clamp keeps the bridge total).
fn clamp_u32(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{role, uast, Node, Positions};

    #[test]
    fn span_text_reads_the_covered_source() {
        let n = Node::new(uast::IDENTIFIER).with_pos(Positions {
            start_offset: 4,
            end_offset: 7,
            ..Positions::default()
        });
        assert_eq!(n.span_text(Some(b"let foo = 1;")), Some("foo"));
        assert_eq!(n.span_text(None), None);
        assert_eq!(
            n.span_text(Some(b"let")),
            None,
            "span past the end is rejected"
        );
    }

    #[test]
    fn from_uast_copies_type_roles_props_and_positions() {
        let child = cf_uast_node::Node {
            node_type: uast::IDENTIFIER.to_string(),
            token: "x".to_string(),
            roles: vec![role::NAME.to_string()],
            ..cf_uast_node::Node::default()
        };
        let mut root = cf_uast_node::Node {
            node_type: uast::FUNCTION.to_string(),
            pos: Some(cf_uast_node::Positions {
                start_line: 1,
                end_line: 3,
                start_offset: 0,
                end_offset: 20,
                ..cf_uast_node::Positions::default()
            }),
            ..cf_uast_node::Node::default()
        };
        root.props.insert("name".to_string(), "x".to_string());
        root.children.push(child);

        let got = Node::from_uast(&root);
        assert_eq!(got.node_type, uast::FUNCTION);
        assert_eq!(got.prop("name"), Some("x"));
        assert_eq!(got.pos.as_ref().map(|p| p.end_offset), Some(20));
        assert_eq!(got.children.len(), 1);
        assert!(got.children[0].has_role(role::NAME));
        assert_eq!(got.children[0].token, "x");
    }

    #[test]
    fn from_uast_survives_deep_trees() {
        const DEPTH: usize = 20_000;
        let mut n = cf_uast_node::Node {
            node_type: uast::IDENTIFIER.to_string(),
            ..cf_uast_node::Node::default()
        };
        for _ in 0..DEPTH {
            n = cf_uast_node::Node {
                node_type: uast::BLOCK.to_string(),
                children: vec![n],
                ..cf_uast_node::Node::default()
            };
        }
        let got = Node::from_uast(&n);
        // The input model drops recursively; keep the test about the bridge.
        std::mem::forget(n);
        let mut count = 0usize;
        got.visit_pre_order(&mut |_| count += 1);
        assert_eq!(count, DEPTH + 1);
    }
}

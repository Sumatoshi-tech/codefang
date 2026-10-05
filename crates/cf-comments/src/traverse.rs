//! UAST traversal helpers used by the comments analyzer: node discovery,
//! declaration naming and comment text geometry.

use cf_uast_node::Node;

use crate::types::{uast, ROLE_NAME, ROLE_TYPE};

/// Whether a walk continues below a node that matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchDescent {
    /// Keep walking into a matched node's children (nested declarations).
    Continue,
    /// Stop at a matched node: its children are part of it (a comment node
    /// lowered with nested comment pieces is one comment).
    Stop,
}

/// Returns, in document (pre-order) order, every node under `root` (root
/// included) whose type is one of `types`.
///
/// With `max_depth = Some(cap)`, only nodes at depth `<= cap` below the root
/// (root at depth 0) are matched; the history `quality` analyzer runs with a
/// cap, the static surface runs uncapped.
pub fn find_nodes_by_type<'a>(
    root: &'a Node,
    types: &[&str],
    max_depth: Option<usize>,
    descent: MatchDescent,
) -> Vec<&'a Node> {
    let mut out = Vec::new();
    walk(
        root,
        &|n| types.contains(&n.node_type.as_str()),
        max_depth,
        descent,
        &mut out,
    );
    out
}

/// Returns, in document order, every node under `root` carrying `role`,
/// stopping at each match (a role-bearing node's children share its lines).
pub fn find_nodes_with_role<'a>(
    root: &'a Node,
    role: &str,
    max_depth: Option<usize>,
) -> Vec<&'a Node> {
    let mut out = Vec::new();
    walk(
        root,
        &|n| has_role(n, role),
        max_depth,
        MatchDescent::Stop,
        &mut out,
    );
    out
}

fn walk<'a>(
    n: &'a Node,
    matches: &dyn Fn(&Node) -> bool,
    remaining: Option<usize>,
    descent: MatchDescent,
    out: &mut Vec<&'a Node>,
) {
    if matches(n) {
        out.push(n);
        if descent == MatchDescent::Stop {
            return;
        }
    }
    let child_remaining = match remaining {
        Some(0) => return,
        Some(r) => Some(r - 1),
        None => None,
    };
    for child in &n.children {
        walk(child, matches, child_remaining, descent, out);
    }
}

/// Whether `n` carries `role`.
#[must_use]
pub fn has_role(n: &Node, role: &str) -> bool {
    n.roles.iter().any(|r| r == role)
}

/// Resolves a declaration's own name: its non-empty `name` prop, else the
/// token of its first direct `Identifier` child that carries the `Name` role
/// without the `Type` role (a type identifier names a type, not the
/// declaration). A token spanning several lines is never a name.
///
/// Returns `None` when the node names nothing itself.
#[must_use]
pub fn declaration_name(node: &Node) -> Option<String> {
    if let Some(value) = node.props.get("name") {
        if !value.is_empty() {
            return Some(value.clone());
        }
    }
    node.children
        .iter()
        .find(|c| {
            c.node_type == uast::IDENTIFIER
                && has_role(c, ROLE_NAME)
                && !has_role(c, ROLE_TYPE)
                && is_single_line(&c.token)
        })
        .map(|c| c.token.clone())
}

fn is_single_line(token: &str) -> bool {
    !token.is_empty() && !token.contains('\n')
}

/// The comment's text: its own token, else the tokens of its nested comment
/// pieces concatenated in order.
#[must_use]
pub fn comment_text(node: &Node) -> String {
    if !node.token.is_empty() {
        return node.token.clone();
    }
    node.children
        .iter()
        .filter(|c| c.node_type == uast::COMMENT)
        .map(comment_text)
        .collect::<String>()
}

/// `(start_line, text_end_line)` of a comment node, or `None` without a
/// position.
///
/// The text end is the last line that holds comment text, decided by the
/// node's own token (a token of N lines ends N-1 lines after the start),
/// else by its nested comment pieces, else by the span end. It never leaves
/// the node's span, so a span that runs onto the following declaration's line
/// (a mapping artefact) cannot move the comment onto that declaration.
#[must_use]
pub fn comment_lines(node: &Node) -> Option<(i64, i64)> {
    let pos = node.pos.as_ref()?;
    let start = pos.start_line as i64;
    let end = (pos.end_line as i64).max(start);
    let text_end = text_end_line(node).unwrap_or(end);
    Some((start, text_end.clamp(start, end)))
}

fn text_end_line(node: &Node) -> Option<i64> {
    let pos = node.pos.as_ref()?;
    if !node.token.is_empty() {
        let extra = node.token.lines().count().max(1) as i64 - 1;
        return Some(pos.start_line as i64 + extra);
    }
    node.children
        .iter()
        .filter(|c| c.node_type == uast::COMMENT)
        .filter_map(text_end_line)
        .max()
}

/// `(start_line, end_line)` of any node, or `None` without a position.
#[must_use]
pub fn node_lines(node: &Node) -> Option<(i64, i64)> {
    let pos = node.pos.as_ref()?;
    let start = pos.start_line as i64;
    Some((start, (pos.end_line as i64).max(start)))
}

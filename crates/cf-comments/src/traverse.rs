//! Generic UAST traversal helpers used by the comments analyzer.
//!
//! These live here (rather than in the shared `cf-analyzers-common` crate) so
//! the crate's only dependency on shared traversal logic is this small,
//! self-contained module; it can be re-pointed at `cf-analyzers-common` when
//! convenient.

use cf_uast_node::Node;

/// Returns, in pre-order, all descendants of `root` (including `root`) whose
/// node type is one of `types`, in document (pre-order, children-in-order)
/// order.
///
/// Note on depth: the STATIC comments surface runs the reference traverser
/// uncapped (`maxDepth <= 0` disables the limit), so this unbounded walk
/// reproduces the static node set byte-for-byte. The HISTORY `quality`
/// analyzer's comments instance runs with `maxDepth = 10` — use
/// [`find_nodes_by_type_capped`] there (measured against the live reference
/// binary: nodes at depth <= 10 from the root, root at depth 0, are matched;
/// deeper nodes are never matched).
pub fn find_nodes_by_type<'a>(root: &'a Node, types: &[&str]) -> Vec<&'a Node> {
    let mut out = Vec::new();
    walk(root, types, None, &mut out);
    out
}

/// Depth-capped [`find_nodes_by_type`]: matches only nodes at depth
/// `<= max_depth` below `root` (`root` at depth 0). The reference traverser
/// filters MATCHES by depth (it still descends), so pruning the walk at the
/// cap is output-equivalent.
pub fn find_nodes_by_type_capped<'a>(
    root: &'a Node,
    types: &[&str],
    max_depth: usize,
) -> Vec<&'a Node> {
    let mut out = Vec::new();
    walk(root, types, Some(max_depth), &mut out);
    out
}

fn walk<'a>(n: &'a Node, types: &[&str], remaining: Option<usize>, out: &mut Vec<&'a Node>) {
    if types.contains(&n.node_type.as_str()) {
        out.push(n);
    }
    let child_remaining = match remaining {
        Some(0) => return,
        Some(r) => Some(r - 1),
        None => None,
    };
    for child in &n.children {
        walk(child, types, child_remaining, out);
    }
}

/// Extracts an entity (function/class/…) name from a target node.
///
/// Resolution order (report contract):
/// 1. the `name` prop — decisive if the KEY EXISTS, **even when empty**: an
///    existing-but-empty value short-circuits to "no name" (the caller's
///    `unknown`) and never falls through to the token/children probes.
/// 2. the node's own `token` (if non-empty);
/// 3. `children[0].token`, else `children[0]`'s `name` prop.
///
/// Returns `None` when no name can be derived (the caller substitutes
/// `"unknown"`).
pub fn extract_entity_name(target: &Node) -> Option<String> {
    // Step 1: the "name" prop — key presence is decisive (see doc comment).
    if let Some(value) = target.props.get("name") {
        return if value.is_empty() {
            None
        } else {
            Some(value.clone())
        };
    }
    // Step 2: the node's own token.
    if !target.token.is_empty() {
        return Some(target.token.clone());
    }
    // Step 3: children[0] — token first, then its "name" prop.
    if let Some(child) = target.children.first() {
        if !child.token.is_empty() {
            return Some(child.token.clone());
        }
        if let Some(value) = child.props.get("name") {
            if !value.is_empty() {
                return Some(value.clone());
            }
        }
    }
    None
}

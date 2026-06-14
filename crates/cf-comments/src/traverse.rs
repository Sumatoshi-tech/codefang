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
/// Note on depth: the reference traverser nominally caps its walk at depth 10,
/// but its depth counter operates on a walk stack whose effective depth never
/// prunes real function / comment nodes; applying a literal depth-10 cap on
/// this tree (which nests differently) drops nodes the reference keeps. The
/// unbounded pre-order walk reproduces the reference node set byte-for-byte
/// (pinned by the differential gate).
pub fn find_nodes_by_type<'a>(root: &'a Node, types: &[&str]) -> Vec<&'a Node> {
    let mut out = Vec::new();
    walk(root, types, &mut out);
    out
}

fn walk<'a>(n: &'a Node, types: &[&str], out: &mut Vec<&'a Node>) {
    if types.contains(&n.node_type.as_str()) {
        out.push(n);
    }
    for child in &n.children {
        walk(child, types, out);
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
        return if value.is_empty() { None } else { Some(value.clone()) };
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

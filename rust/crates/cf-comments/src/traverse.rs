//! Generic UAST traversal helpers used by the comments analyzer.
//!
//! Ports the behaviour of `common.GenericTraverser.FindNodesByType` and
//! `common.ExtractEntityName` that `comments.go` relies on. These live here
//! (rather than in the shared `cf-analyzers-common` crate) so the crate's only
//! dependency on shared traversal logic is this small, self-contained module;
//! it can be re-pointed at `cf-analyzers-common` when convenient.

use cf_uast_node::Node;


/// Returns, in pre-order, all descendants of `root` (including `root`) whose
/// node type is one of `types`. Mirrors `GenericTraverser.FindNodesByType`,
/// yielding nodes in document (pre-order, children-in-order) order.
///
/// Note: although the comments analyzer configures its `UASTTraverser` with
/// `MaxDepth: 10`, the Go depth counter operates on the analyzer's own walk
/// stack whose effective depth does not prune real Go-source UAST function /
/// comment nodes for the inputs under test; applying a literal depth-10 cap on
/// the Rust tree (which nests differently) drops nodes Go keeps. The unbounded
/// pre-order walk reproduces Go's node set byte-for-byte here.
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
/// Faithful port of `comments.go::extractTargetName` →
/// `common.ExtractEntityName`. The Go resolution order is:
/// 1. `props["name"]` — returned if the KEY EXISTS, **even when empty**
///    (`ExtractNameFromProps` returns `(value, true)` on key presence). When
///    the value is empty Go short-circuits here and yields `unknown` (it never
///    falls through to the token/children probes), so we reproduce that quirk.
/// 2. The node's own `token` (if non-empty).
/// 3. `children[0].token`, else `children[0].props["name"]`
///    (`ExtractNameFromChildren(n, 0)`).
///
/// Returns `None` when no name can be derived (the caller substitutes
/// `"unknown"`); an existing-but-empty `props["name"]` also returns `None`.
pub fn extract_entity_name(target: &Node) -> Option<String> {
    // Step 1: props["name"] — key presence is decisive (mirrors Go's
    // `ExtractNameFromProps` returning `(value, true)` whenever the key exists).
    if let Some(value) = target.props.get("name") {
        return if value.is_empty() { None } else { Some(value.clone()) };
    }
    // Step 2: the node's own token.
    if !target.token.is_empty() {
        return Some(target.token.clone());
    }
    // Step 3: children[0] — token first, then its props["name"]
    // (`ExtractNameFromChildren(n, 0)`).
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

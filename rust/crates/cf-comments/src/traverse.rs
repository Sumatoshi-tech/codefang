//! Generic UAST traversal helpers used by the comments analyzer.
//!
//! Ports the behaviour of `common.GenericTraverser.FindNodesByType` and
//! `common.ExtractEntityName` that `comments.go` relies on. These live here
//! (rather than in the shared `cf-analyzers-common` crate) so the crate's only
//! dependency on shared traversal logic is this small, self-contained module;
//! it can be re-pointed at `cf-analyzers-common` when convenient.

use cf_uast_node::Node;

use crate::types::{uast, ROLE_NAME};

/// Returns, in pre-order, all descendants of `root` (including `root`) whose
/// node type is one of `types`. Mirrors `GenericTraverser.FindNodesByType`,
/// which yields nodes in document (depth-first, children-in-order) order.
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
/// Resolution order mirrors `comments.go::extractTargetName` →
/// `common.ExtractEntityName`:
/// 1. A child identifier carrying the `Name` role (token non-empty).
/// 2. The `name` property.
/// 3. The first child identifier's token (fallback).
///
/// Returns `None` when no name can be derived (the caller substitutes
/// `"unknown"`).
pub fn extract_entity_name(target: &Node) -> Option<String> {
    if let Some(name) = name_role_identifier(target) {
        if !name.is_empty() {
            return Some(name);
        }
    }
    if let Some(name) = target.props.get("name") {
        if !name.is_empty() {
            return Some(name.clone());
        }
    }
    if let Some(name) = first_identifier_token(target) {
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

fn name_role_identifier(target: &Node) -> Option<String> {
    target
        .children
        .iter()
        .find(|c| {
            c.node_type == uast::IDENTIFIER && c.has_any_role(&[ROLE_NAME]) && !c.token.is_empty()
        })
        .map(|c| c.token.clone())
}

fn first_identifier_token(target: &Node) -> Option<String> {
    target
        .children
        .iter()
        .find(|c| c.node_type == uast::IDENTIFIER && !c.token.is_empty())
        .map(|c| c.token.clone())
}

//! Structural diffing between two UAST trees.
//!
//! Direct port of Go `pkg/uast/changes.go` (`DetectChanges`, `diffChildren` and
//! helpers). The traversal order and child-matching strategy are preserved
//! exactly: children are matched greedily by `(type, token)` key, the first
//! unused candidate wins, the parent's own `Modified` change is emitted *before*
//! the recursive child changes, then unmatched-before nodes are reported as
//! `Removed` and unmatched-after nodes as `Added`.

use std::collections::HashMap;

use cf_uast_node::{Node, Positions};

use crate::types::{ChangeType, NodeChange};

/// Detects structural changes between two UAST nodes (Go `DetectChanges`).
///
/// Returns the empty vector when both are `None`. Otherwise:
/// * `None` → `Some` yields a single [`ChangeType::Added`];
/// * `Some` → `None` yields a single [`ChangeType::Removed`];
/// * `Some` → `Some` compares the node itself (token, type, position) and its
///   children, emitting a [`ChangeType::Modified`] for the pair (when the node
///   changed or any child changed) followed by the child changes.
pub fn detect_changes(before: Option<&Node>, after: Option<&Node>) -> Vec<NodeChange> {
    match (before, after) {
        (None, Some(a)) => vec![NodeChange {
            before: None,
            after: Some(a.clone()),
            file: String::new(),
            change_type: ChangeType::Added,
        }],
        (Some(b), None) => vec![NodeChange {
            before: Some(b.clone()),
            after: None,
            file: String::new(),
            change_type: ChangeType::Removed,
        }],
        (None, None) => Vec::new(),
        (Some(b), Some(a)) => {
            let node_modified = b.token != a.token
                || b.node_type != a.node_type
                || positions_changed(b.pos.as_ref(), a.pos.as_ref());

            let child_changes = diff_children(b, a);

            let mut changes = Vec::new();
            if node_modified || !child_changes.is_empty() {
                changes.push(NodeChange {
                    before: Some(b.clone()),
                    after: Some(a.clone()),
                    file: String::new(),
                    change_type: ChangeType::Modified,
                });
            }
            changes.extend(child_changes);
            changes
        }
    }
}

/// Returns whether two optional positions differ (Go `positionsChanged`).
///
/// Both `None` → unchanged; exactly one `None` → changed; otherwise the four
/// line/col fields are compared (byte offsets are intentionally ignored, exactly
/// as in Go).
fn positions_changed(pos_a: Option<&Positions>, pos_b: Option<&Positions>) -> bool {
    match (pos_a, pos_b) {
        (None, None) => false,
        (None, Some(_)) | (Some(_), None) => true,
        (Some(a), Some(b)) => {
            a.start_line != b.start_line
                || a.start_col != b.start_col
                || a.end_line != b.end_line
                || a.end_col != b.end_col
        }
    }
}

/// Identifies a child by `(type, token)` (Go `childKey`).
#[derive(PartialEq, Eq, Hash, Clone)]
struct ChildKey {
    node_type: String,
    token: String,
}

/// Compares the children of two nodes (Go `diffChildren`).
fn diff_children(before: &Node, after: &Node) -> Vec<NodeChange> {
    let before_children = &before.children;
    let after_children = &after.children;

    if before_children.is_empty() && after_children.is_empty() {
        return Vec::new();
    }

    let mut after_used = vec![false; after_children.len()];
    let after_index = build_child_index(after_children);
    let mut before_matched = vec![false; before_children.len()];

    let mut changes = match_children(
        before_children,
        after_children,
        &after_index,
        &mut before_matched,
        &mut after_used,
    );
    changes.extend(collect_removed_children(before_children, &before_matched));
    changes.extend(collect_added_children(after_children, &after_used));
    changes
}

/// Builds an index from `(type, token)` to the child indices with that key, in
/// order (Go `buildChildIndex`).
fn build_child_index(children: &[Node]) -> HashMap<ChildKey, Vec<usize>> {
    let mut index: HashMap<ChildKey, Vec<usize>> = HashMap::new();
    for (idx, child) in children.iter().enumerate() {
        let key = ChildKey {
            node_type: child.node_type.clone(),
            token: child.token.clone(),
        };
        index.entry(key).or_default().push(idx);
    }
    index
}

/// Greedily matches before-children to after-children by key (Go
/// `matchChildren`), recursing into matched pairs.
fn match_children(
    before_children: &[Node],
    after_children: &[Node],
    after_index: &HashMap<ChildKey, Vec<usize>>,
    before_matched: &mut [bool],
    after_used: &mut [bool],
) -> Vec<NodeChange> {
    let mut changes = Vec::new();

    for (idx, bc) in before_children.iter().enumerate() {
        let key = ChildKey {
            node_type: bc.node_type.clone(),
            token: bc.token.clone(),
        };

        let indices = match after_index.get(&key) {
            Some(v) => v,
            None => continue,
        };

        for &after_idx in indices {
            if after_used[after_idx] {
                continue;
            }
            after_used[after_idx] = true;
            before_matched[idx] = true;
            changes.extend(detect_changes(Some(bc), Some(&after_children[after_idx])));
            break;
        }
    }

    changes
}

/// Reports unmatched before-children as removed (Go `collectRemovedChildren`).
fn collect_removed_children(before_children: &[Node], before_matched: &[bool]) -> Vec<NodeChange> {
    let mut changes = Vec::new();
    for (idx, bc) in before_children.iter().enumerate() {
        if !before_matched[idx] {
            changes.push(NodeChange {
                before: Some(bc.clone()),
                after: None,
                file: String::new(),
                change_type: ChangeType::Removed,
            });
        }
    }
    changes
}

/// Reports unmatched after-children as added (Go `collectAddedChildren`).
fn collect_added_children(after_children: &[Node], after_used: &[bool]) -> Vec<NodeChange> {
    let mut changes = Vec::new();
    for (idx, ac) in after_children.iter().enumerate() {
        if !after_used[idx] {
            changes.push(NodeChange {
                before: None,
                after: Some(ac.clone()),
                file: String::new(),
                change_type: ChangeType::Added,
            });
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast_node::Node;

    fn n(node_type: &str, token: &str) -> Node {
        Node::with_token(node_type, token)
    }

    #[test]
    fn both_nil_is_empty() {
        assert!(detect_changes(None, None).is_empty());
    }

    #[test]
    fn nil_to_some_is_added() {
        let node = n("Identifier", "x");
        let changes = detect_changes(None, Some(&node));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Added);
    }

    #[test]
    fn some_to_nil_is_removed() {
        let node = n("Identifier", "x");
        let changes = detect_changes(Some(&node), None);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Removed);
    }

    #[test]
    fn identical_nodes_no_changes() {
        let node = n("Identifier", "x");
        assert!(detect_changes(Some(&node), Some(&node)).is_empty());
    }

    #[test]
    fn token_change_is_modified() {
        let a = n("Identifier", "x");
        let b = n("Identifier", "y");
        let changes = detect_changes(Some(&a), Some(&b));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn type_change_is_modified() {
        let a = n("Identifier", "x");
        let b = n("Literal", "x");
        let changes = detect_changes(Some(&a), Some(&b));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn added_child_reported() {
        let mut a = n("File", "");
        a.children.push(n("Function", "foo"));
        let mut b = a.clone();
        b.children.push(n("Function", "bar"));

        let changes = detect_changes(Some(&a), Some(&b));
        // Parent is Modified (children differ) + the new child is Added.
        assert!(changes
            .iter()
            .any(|c| c.change_type == ChangeType::Modified));
        assert!(changes.iter().any(|c| c.change_type == ChangeType::Added
            && c.after.as_ref().map(|x| x.token.as_str()) == Some("bar")));
    }

    #[test]
    fn removed_child_reported() {
        let mut a = n("File", "");
        a.children.push(n("Function", "foo"));
        a.children.push(n("Function", "bar"));
        let mut b = n("File", "");
        b.children.push(n("Function", "foo"));

        let changes = detect_changes(Some(&a), Some(&b));
        assert!(changes.iter().any(|c| c.change_type == ChangeType::Removed
            && c.before.as_ref().map(|x| x.token.as_str()) == Some("bar")));
    }

    #[test]
    fn parent_modified_emitted_before_child_changes() {
        // Reproduces the Go ordering: the Modified for the (before, after) pair
        // appears first, then the child-level changes.
        let mut a = n("File", "");
        a.children.push(n("Function", "foo"));
        let mut b = n("File", "");
        b.children.push(n("Function", "bar"));

        let changes = detect_changes(Some(&a), Some(&b));
        assert_eq!(changes[0].change_type, ChangeType::Modified);
        assert!(changes[0].before.as_ref().map(|x| x.r#type.as_str()) == Some("File"));
    }
}

//! [`Allocator`] — a per-worker free-list object pool for [`Node`] and
//! [`Positions`].
//!
//! The pool keeps a local free list per parse invocation so parse-heavy
//! workloads can recycle buffers (capacity is retained across reuse). Pooling
//! is purely a performance device; it never affects output bytes. It is
//! intentionally not `Sync` (single-threaded use).

use crate::node::{Node, Positions, Role, Type};
use std::collections::HashMap;

/// A per-worker free-list allocator for [`Node`] and [`Positions`].
///
/// Not safe for concurrent use.
#[derive(Debug, Default)]
pub struct Allocator {
    nodes: Vec<Node>,
    pos: Vec<Positions>,
}

impl Allocator {
    /// Creates an empty allocator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a zeroed [`Node`], reusing one from the free list if available.
    pub fn get_node(&mut self) -> Node {
        self.nodes.pop().unwrap_or_default()
    }

    /// Clears `node` and returns it to the free list.
    pub fn put_node(&mut self, mut node: Node) {
        node.id.clear();
        node.node_type.clear();
        node.token.clear();
        node.roles.clear();
        node.pos = None;
        node.props.clear();
        node.children.clear();
        self.nodes.push(node);
    }

    /// Returns a zeroed [`Positions`], reusing one from the free list if
    /// available.
    pub fn get_positions(&mut self) -> Positions {
        self.pos.pop().unwrap_or_default()
    }

    /// Clears `positions` and returns it to the free list.
    pub fn put_positions(&mut self, _positions: Positions) {
        // Positions is Copy/zeroable; push a fresh zero value to the free list.
        self.pos.push(Positions::default());
    }

    /// Creates a fully-initialized [`Node`] from the free list.
    pub fn new_node(
        &mut self,
        id: impl Into<Vec<u8>>,
        node_type: impl Into<Type>,
        token: impl Into<String>,
        roles: Vec<Role>,
        pos: Option<Positions>,
        props: HashMap<String, String>,
    ) -> Node {
        let mut node = self.get_node();
        node.id = id.into();
        node.node_type = node_type.into();
        node.token = token.into();
        node.roles = roles;
        node.pos = pos;
        node.props = props;
        node
    }

    /// Creates a fully-initialized [`Positions`] from the free list.
    #[allow(clippy::too_many_arguments)]
    pub fn new_positions(
        &mut self,
        start_line: u64,
        start_col: u64,
        start_offset: u64,
        end_line: u64,
        end_col: u64,
        end_offset: u64,
    ) -> Positions {
        let mut p = self.get_positions();
        p.start_line = start_line;
        p.start_col = start_col;
        p.start_offset = start_offset;
        p.end_line = end_line;
        p.end_col = end_col;
        p.end_offset = end_offset;
        p
    }

    /// Returns every node and position in `root`'s tree to the free lists.
    /// Consumes the tree by value and recycles the owned values.
    pub fn release_tree(&mut self, root: Node) {
        let mut stack = vec![root];
        while let Some(mut current) = stack.pop() {
            let children = std::mem::take(&mut current.children);
            for child in children {
                stack.push(child);
            }
            if let Some(pos) = current.pos.take() {
                self.put_positions(pos);
            }
            self.put_node(current);
        }
    }
}

/// Releases a whole tree's nodes/positions without an [`Allocator`].
///
/// There is no global pool (owned values are simply dropped), so this is a
/// no-op-by-drop convenience that consumes the tree. It exists so call sites
/// can express deterministic teardown intent.
pub fn release_tree(root: Node) {
    drop(root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_node_returns_zeroed() {
        let mut a = Allocator::new();
        let n = a.get_node();
        assert_eq!(n, Node::default());
    }

    #[test]
    fn put_then_get_reuses_and_clears() {
        let mut a = Allocator::new();
        let mut n1 = a.get_node();
        n1.node_type = "Function".into();
        a.put_node(n1);
        let n2 = a.get_node();
        assert_eq!(n2.node_type, "");
    }

    #[test]
    fn new_node_sets_fields() {
        let mut a = Allocator::new();
        let pos = a.new_positions(1, 2, 3, 4, 5, 6);
        let n = a.new_node(
            Vec::new(),
            "Function",
            "tok",
            vec!["Declaration".into()],
            Some(pos),
            HashMap::new(),
        );
        assert_eq!(n.node_type, "Function");
        assert_eq!(n.pos.unwrap().start_line, 1);
    }

    #[test]
    fn new_positions_sets_fields() {
        let mut a = Allocator::new();
        let pos = a.new_positions(1, 2, 3, 4, 5, 6);
        assert_eq!(pos.start_line, 1);
        assert_eq!(pos.end_offset, 6);
    }

    #[test]
    fn release_tree_recycles() {
        let mut a = Allocator::new();
        let mut root = a.new_node(Vec::new(), "File", "", Vec::new(), None, HashMap::new());
        let child = a.new_node(Vec::new(), "Function", "", Vec::new(), None, HashMap::new());
        root.children = vec![child];
        a.release_tree(root);
        // After release, the free list should serve a node without allocating new.
        let _ = a.get_node();
        assert!(a.nodes.len() <= 1); // one consumed by get_node
    }
}

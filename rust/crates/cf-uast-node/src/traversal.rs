//! Tree traversal and transformation. Ported from the traversal helpers in
//! `node.go` (`Find`, `VisitPreOrder`, `VisitPostOrder`, `PreOrder`,
//! `Ancestors`, `Transform`, `TransformInPlace`).
//!
//! The Go implementation hand-rolls iterative stacks with depth-limit fallbacks
//! purely as a performance/stack-safety optimization; the *observable* order is
//! the same as a straightforward pre/post-order walk, so the Rust port uses
//! plain iterative stacks that produce identical visitation order.

use crate::node::Node;

impl Node {
    /// Returns references to all nodes (including self) for which `predicate`
    /// returns `true`, in pre-order. Mirrors Go's `Find`.
    pub fn find<F: Fn(&Node) -> bool>(&self, predicate: F) -> Vec<&Node> {
        let mut result = Vec::new();
        let mut stack: Vec<&Node> = vec![self];
        while let Some(curr) = stack.pop() {
            if predicate(curr) {
                result.push(curr);
            }
            // Push children reversed so left-to-right pre-order pops first.
            for child in curr.children.iter().rev() {
                stack.push(child);
            }
        }
        result
    }

    /// Visits every node in pre-order (root, then children left-to-right),
    /// invoking `f` on each. Mirrors Go's `VisitPreOrder`.
    pub fn visit_pre_order<F: FnMut(&Node)>(&self, mut f: F) {
        let mut stack: Vec<&Node> = vec![self];
        while let Some(curr) = stack.pop() {
            f(curr);
            for child in curr.children.iter().rev() {
                stack.push(child);
            }
        }
    }

    /// Visits every node in post-order (children left-to-right, then root).
    /// Mirrors Go's `VisitPostOrder`.
    pub fn visit_post_order<F: FnMut(&Node)>(&self, mut f: F) {
        // Frame holds a node and the index of the next child to descend into.
        let mut stack: Vec<(&Node, usize)> = vec![(self, 0)];
        while let Some(&mut (node, ref mut idx)) = stack.last_mut() {
            if *idx < node.children.len() {
                let child = &node.children[*idx];
                *idx += 1;
                stack.push((child, 0));
            } else {
                f(node);
                stack.pop();
            }
        }
    }

    /// Returns all nodes (including self) in pre-order as an owned `Vec` of
    /// references. The Go API returns a goroutine-backed channel (`PreOrder()`);
    /// the eager `Vec` is the idiomatic Rust equivalent with identical order.
    pub fn pre_order(&self) -> Vec<&Node> {
        // Built directly rather than via `visit_pre_order` so the returned
        // references can outlive a closure (closures invariant over `&Node`).
        let mut out = Vec::new();
        let mut stack: Vec<&Node> = vec![self];
        while let Some(curr) = stack.pop() {
            out.push(curr);
            for child in curr.children.iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    /// Returns the chain of ancestors from the root down to the parent of
    /// `target` (empty if `target` is the root, `None` if not found). Mirrors
    /// Go's `Ancestors`, comparing by structural equality (Go compares by
    /// pointer; for value trees structural identity is the faithful analogue).
    pub fn ancestors<'a>(&'a self, target: &Node) -> Option<Vec<&'a Node>> {
        // DFS carrying the path of ancestors to each node.
        let mut stack: Vec<(&Node, Vec<&Node>)> = vec![(self, Vec::new())];
        while let Some((node, path)) = stack.pop() {
            if std::ptr::eq(node, target) || node == target {
                return Some(path);
            }
            if node.children.is_empty() {
                continue;
            }
            let mut child_path = path;
            child_path.push(node);
            for child in node.children.iter().rev() {
                stack.push((child, child_path.clone()));
            }
        }
        None
    }

    /// Mutates the tree in place in pre-order. `f` returns whether to continue
    /// descending into the node's children. Mirrors Go's `TransformInPlace`.
    pub fn transform_in_place<F: FnMut(&mut Node) -> bool>(&mut self, f: &mut F) {
        let descend = f(self);
        if descend {
            for child in &mut self.children {
                child.transform_in_place(f);
            }
        }
    }

    /// Returns a new tree where each node is replaced by `f(node)` in post-order
    /// (children transformed first). Mirrors Go's `Transform`, which deep-copies
    /// then applies the function bottom-up.
    pub fn transform<F: Fn(Node) -> Node>(&self, f: &F) -> Node {
        let mut copy = self.clone();
        let transformed_children: Vec<Node> =
            copy.children.iter().map(|c| c.transform(f)).collect();
        copy.children = transformed_children;
        f(copy)
    }
}

#[cfg(test)]
mod tests {
    use crate::node::Node;

    fn file_with_two_functions() -> Node {
        let mut root = Node::with_token("File", "");
        root.add_child(Node::with_token("Function", "a"));
        root.add_child(Node::with_token("Function", "b"));
        root
    }

    #[test]
    fn find_matches_predicate() {
        // Mirrors Go TestNode_Find (3 children, 2 Functions).
        let mut root = Node::with_token("File", "");
        root.add_child(Node::with_token("Function", "a"));
        root.add_child(Node::with_token("Function", "b"));
        root.add_child(Node::with_token("Variable", "c"));
        let fns = root.find(|n| n.node_type == "Function");
        assert_eq!(fns.len(), 2);
    }

    #[test]
    fn visit_pre_order_visits_root_then_children() {
        // Mirrors Go TestNode_VisitPreOrder.
        let root = file_with_two_functions();
        let mut visited = Vec::new();
        root.visit_pre_order(|n| visited.push(n.node_type.clone()));
        assert_eq!(visited, vec!["File", "Function", "Function"]);
    }

    #[test]
    fn visit_post_order_visits_children_then_root() {
        let root = file_with_two_functions();
        let mut visited = Vec::new();
        root.visit_post_order(|n| visited.push(n.token.clone()));
        // children "a","b" (left-to-right) then root token "".
        assert_eq!(visited, vec!["a", "b", ""]);
    }

    #[test]
    fn pre_order_collects_all_nodes() {
        let root = file_with_two_functions();
        assert_eq!(root.pre_order().len(), 3);
    }

    #[test]
    fn ancestors_returns_path() {
        let root = file_with_two_functions();
        let target = root.children[1].clone();
        let path = root.ancestors(&target).expect("found");
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].node_type, "File");
    }

    #[test]
    fn transform_in_place_mutates() {
        let mut root = Node::with_token("File", "");
        root.add_child(Node::with_token("Comment", "hi"));
        root.transform_in_place(&mut |n: &mut Node| {
            if n.node_type == "Comment" {
                n.token = String::new();
            }
            true
        });
        assert_eq!(root.children[0].token, "");
    }

    #[test]
    fn transform_builds_new_tree() {
        let root = file_with_two_functions();
        let out = root.transform(&|mut n: Node| {
            n.token = format!("{}!", n.token);
            n
        });
        assert_eq!(out.token, "!");
        assert_eq!(out.children[0].token, "a!");
    }
}

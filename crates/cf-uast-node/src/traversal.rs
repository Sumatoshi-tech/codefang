//! Tree traversal and transformation: `find`, `visit_pre_order`,
//! `visit_post_order`, `pre_order`, `ancestors`, `transform`,
//! `transform_in_place`.
//!
//! All walks use plain iterative stacks; visitation order (pre/post-order,
//! children left-to-right) is observable behavior relied on by analyzers.

use crate::node::Node;

impl Node {
    /// Returns references to all nodes (including self) for which `predicate`
    /// returns `true`, in pre-order.
    pub fn find<F: Fn(&Self) -> bool>(&self, predicate: F) -> Vec<&Self> {
        let mut result = Vec::new();
        let mut stack: Vec<&Self> = vec![self];
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
    /// invoking `f` on each.
    pub fn visit_pre_order<F: FnMut(&Self)>(&self, mut f: F) {
        let mut stack: Vec<&Self> = vec![self];
        while let Some(curr) = stack.pop() {
            f(curr);
            for child in curr.children.iter().rev() {
                stack.push(child);
            }
        }
    }

    /// Visits every node in post-order (children left-to-right, then root).
    pub fn visit_post_order<F: FnMut(&Self)>(&self, mut f: F) {
        // Frame holds a node and the index of the next child to descend into.
        let mut stack: Vec<(&Self, usize)> = vec![(self, 0)];
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
    /// references — the eager counterpart of [`Node::visit_pre_order`].
    #[must_use]
    pub fn pre_order(&self) -> Vec<&Self> {
        // Built directly rather than via `visit_pre_order` so the returned
        // references can outlive a closure (closures invariant over `&Node`).
        let mut out = Vec::new();
        let mut stack: Vec<&Self> = vec![self];
        while let Some(curr) = stack.pop() {
            out.push(curr);
            for child in curr.children.iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    /// Returns the chain of ancestors from the root down to the parent of
    /// `target` (empty if `target` is the root, `None` if not found). Matches
    /// by pointer identity first, then structural equality (for value trees
    /// structural identity is the analogue of reference identity).
    #[must_use]
    pub fn ancestors<'a>(&'a self, target: &Self) -> Option<Vec<&'a Self>> {
        // DFS carrying the path of ancestors to each node.
        let mut stack: Vec<(&Self, Vec<&Self>)> = vec![(self, Vec::new())];
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
    /// descending into the node's children.
    pub fn transform_in_place<F: FnMut(&mut Self) -> bool>(&mut self, f: &mut F) {
        let descend = f(self);
        if descend {
            for child in &mut self.children {
                child.transform_in_place(f);
            }
        }
    }

    /// Returns a new tree where each node is replaced by `f(node)` in
    /// post-order: the tree is deep-copied and the function applied bottom-up
    /// (children transformed first).
    #[must_use]
    pub fn transform<F: Fn(Self) -> Self>(&self, f: &F) -> Self {
        let mut copy = self.clone();
        let transformed_children: Vec<Self> =
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
        let mut root = Node::with_token("File", "");
        root.add_child(Node::with_token("Function", "a"));
        root.add_child(Node::with_token("Function", "b"));
        root.add_child(Node::with_token("Variable", "c"));
        let fns = root.find(|n| n.node_type == "Function");
        assert_eq!(fns.len(), 2);
    }

    #[test]
    fn visit_pre_order_visits_root_then_children() {
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

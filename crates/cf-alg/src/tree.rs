//! Iterative tree traversal.

/// Initial capacity of the explicit traversal stack.
const DEFAULT_TREE_STACK_CAP: usize = 32;

/// Performs an iterative pre-order DFS over a tree.
///
/// `children` returns the children of a node; `visit` is called for each node
/// with its depth (the root has depth `0`). An empty children slice terminates
/// the branch.
///
/// Children are pushed onto the explicit stack in reverse so they are visited
/// left-to-right (pre-order); downstream callers rely on this order.
/// `T: Clone` because node values are copied onto the work stack.
///
/// # Examples
///
/// ```
/// use cf_alg::traverse_tree;
///
/// #[derive(Clone)]
/// struct Node {
///     name: &'static str,
///     children: Vec<Node>,
/// }
///
/// // Tree: a -> {b -> {d}, c}.
/// let tree = Node {
///     name: "a",
///     children: vec![
///         Node { name: "b", children: vec![Node { name: "d", children: vec![] }] },
///         Node { name: "c", children: vec![] },
///     ],
/// };
///
/// let mut order = Vec::new();
/// traverse_tree(tree, |n| n.children.clone(), |n, depth| order.push((n.name, depth)));
/// assert_eq!(order, vec![("a", 0), ("b", 1), ("d", 2), ("c", 1)]);
/// ```
pub fn traverse_tree<T, C, V>(root: T, mut children: C, mut visit: V)
where
    T: Clone,
    C: FnMut(&T) -> Vec<T>,
    V: FnMut(&T, usize),
{
    let mut stack: Vec<(T, usize)> = Vec::with_capacity(DEFAULT_TREE_STACK_CAP);
    stack.push((root, 0));

    while let Some((node, depth)) = stack.pop() {
        visit(&node, depth);

        let kids = children(&node);
        let child_depth = depth + 1;

        for kid in kids.into_iter().rev() {
            stack.push((kid, child_depth));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct TestNode {
        name: &'static str,
        children: Vec<TestNode>,
    }

    impl TestNode {
        fn leaf(name: &'static str) -> Self {
            Self {
                name,
                children: Vec::new(),
            }
        }

        fn with_children(name: &'static str, children: Vec<TestNode>) -> Self {
            Self { name, children }
        }
    }

    fn children_of(n: &TestNode) -> Vec<TestNode> {
        n.children.clone()
    }

    #[test]
    fn single_node() {
        let root = TestNode::leaf("root");
        let mut visited = Vec::new();
        traverse_tree(root, children_of, |n, depth| {
            visited.push(n.name);
            assert_eq!(depth, 0);
        });
        assert_eq!(visited, vec!["root"]);
    }

    #[test]
    fn pre_order() {
        // Tree: a -> {b -> {d}, c}.
        let d = TestNode::leaf("d");
        let b = TestNode::with_children("b", vec![d]);
        let c = TestNode::leaf("c");
        let a = TestNode::with_children("a", vec![b, c]);

        let mut visited = Vec::new();
        traverse_tree(a, children_of, |n, _| visited.push(n.name));
        assert_eq!(visited, vec!["a", "b", "d", "c"]);
    }

    #[test]
    fn depth_tracking() {
        use std::collections::HashMap;

        let d = TestNode::leaf("d");
        let b = TestNode::with_children("b", vec![d]);
        let c = TestNode::leaf("c");
        let a = TestNode::with_children("a", vec![b, c]);

        let mut depths: HashMap<&'static str, usize> = HashMap::new();
        traverse_tree(a, children_of, |n, depth| {
            depths.insert(n.name, depth);
        });

        assert_eq!(depths["a"], 0);
        assert_eq!(depths["b"], 1);
        assert_eq!(depths["c"], 1);
        assert_eq!(depths["d"], 2);
    }

    #[test]
    fn nil_children() {
        let root = TestNode::leaf("root");
        let mut count = 0;
        traverse_tree(root, children_of, |_, _| count += 1);
        assert_eq!(count, 1);
    }

    #[test]
    fn empty_children_terminates_branch() {
        let leaf = TestNode::with_children("leaf", Vec::new());
        let root = TestNode::with_children("root", vec![leaf]);

        let mut visited = Vec::new();
        traverse_tree(root, children_of, |n, _| visited.push(n.name));
        assert_eq!(visited, vec!["root", "leaf"]);
    }

    #[test]
    fn visit_filters_by_depth() {
        let d = TestNode::leaf("d");
        let b = TestNode::with_children("b", vec![d]);
        let c = TestNode::leaf("c");
        let a = TestNode::with_children("a", vec![b, c]);

        const MAX_DEPTH: usize = 1;
        let mut visited = Vec::new();
        traverse_tree(a, children_of, |n, depth| {
            if depth <= MAX_DEPTH {
                visited.push(n.name);
            }
        });

        assert!(visited.contains(&"a"));
        assert!(visited.contains(&"b"));
        assert!(visited.contains(&"c"));
        assert!(!visited.contains(&"d"));
    }

    #[test]
    fn int_tree() {
        // Non-pointer value types.
        #[derive(Clone)]
        struct IntNode {
            val: i32,
            children: Vec<IntNode>,
        }

        let root = IntNode {
            val: 1,
            children: vec![
                IntNode {
                    val: 2,
                    children: Vec::new(),
                },
                IntNode {
                    val: 3,
                    children: vec![IntNode {
                        val: 4,
                        children: Vec::new(),
                    }],
                },
            ],
        };

        let mut sum = 0;
        traverse_tree(root, |n: &IntNode| n.children.clone(), |n, _| sum += n.val);
        assert_eq!(sum, 10);
    }

    #[test]
    fn wide_tree() {
        const WIDTH: usize = 100;
        let children: Vec<TestNode> = (0..WIDTH).map(|_| TestNode::leaf("child")).collect();
        let root = TestNode::with_children("root", children);

        let mut count = 0;
        traverse_tree(root, children_of, |_, _| count += 1);
        assert_eq!(count, WIDTH + 1);
    }
}

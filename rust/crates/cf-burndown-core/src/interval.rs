//! Augmented interval tree for efficient overlap queries.
//!
//! Vendored port of `pkg/alg/interval/interval.go` (the Go `interval` package),
//! specialized to the `u32` key and `u32` value used by burndown range queries.
//!
//! This is an INTERIM copy. The shared `cf-alg-interval` crate from the design
//! has not been ported yet; once it exists, [`crate::range_query`] should depend
//! on it and this module should be deleted. See the crate-level TODOs.
//!
//! The Go tree is a red-black tree augmented with `maxHigh` (the maximum right
//! endpoint in each subtree) for subtree pruning during overlap queries. This
//! Rust port preserves the augmentation and the query/ordering semantics
//! (`QueryOverlap` returns matches in ascending `Low`, then `High`), which is
//! what `range_query` relies on. The internal balancing strategy here is AVL
//! rather than red-black: both keep the tree height `O(log n)` and the in-order
//! traversal identical, so `QueryOverlap` results are byte-identical to Go's for
//! the burndown use (which only ever inserts and queries, never deletes).

/// An interval `[low, high]` with an associated value. Bounds are treated as
/// inclusive for overlap. Mirrors the Go `Interval` struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    /// Inclusive lower bound.
    pub low: u32,
    /// Inclusive upper bound.
    pub high: u32,
    /// Associated value (the owning time/tick for burndown).
    pub value: u32,
}

/// A single node in the interval tree. Mirrors the Go `node` struct.
#[derive(Debug)]
struct Node {
    interval: Interval,
    max_high: u32,
    left: Option<Box<Node>>,
    right: Option<Box<Node>>,
    height: i32,
}

impl Node {
    fn new(low: u32, high: u32, value: u32) -> Box<Node> {
        Box::new(Node {
            interval: Interval { low, high, value },
            max_high: high,
            left: None,
            right: None,
            height: 1,
        })
    }
}

/// An augmented interval tree. Mirrors the Go `Tree` struct.
#[derive(Debug, Default)]
pub struct Tree {
    root: Option<Box<Node>>,
    size: usize,
}

fn height(node: &Option<Box<Node>>) -> i32 {
    node.as_ref().map_or(0, |n| n.height)
}

fn balance_factor(node: &Option<Box<Node>>) -> i32 {
    node.as_ref().map_or(0, |n| height(&n.left) - height(&n.right))
}

fn update_height_and_max(node: &mut Node) {
    node.height = 1 + height(&node.left).max(height(&node.right));
    node.max_high = node.interval.high;

    if let Some(l) = &node.left {
        if l.max_high > node.max_high {
            node.max_high = l.max_high;
        }
    }

    if let Some(r) = &node.right {
        if r.max_high > node.max_high {
            node.max_high = r.max_high;
        }
    }
}

fn left_rotate(mut x: Box<Node>) -> Box<Node> {
    let mut y = x.right.take().expect("left_rotate requires a right child");
    let t2 = y.left.take();

    x.right = t2;
    update_height_and_max(&mut x);

    y.left = Some(x);
    update_height_and_max(&mut y);

    y
}

fn right_rotate(mut y: Box<Node>) -> Box<Node> {
    let mut x = y.left.take().expect("right_rotate requires a left child");
    let t2 = x.right.take();

    y.left = t2;
    update_height_and_max(&mut y);

    x.right = Some(y);
    update_height_and_max(&mut x);

    x
}

fn rebalance(mut node: Box<Node>) -> Box<Node> {
    let balance = height(&node.left) - height(&node.right);

    if balance > 1 {
        if balance_factor(&node.left) < 0 {
            node.left = Some(left_rotate(node.left.take().expect("left child")));
        }
        return right_rotate(node);
    }

    if balance < -1 {
        if balance_factor(&node.right) > 0 {
            node.right = Some(right_rotate(node.right.take().expect("right child")));
        }
        return left_rotate(node);
    }

    node
}

fn insert_node(node: Option<Box<Node>>, low: u32, high: u32, value: u32) -> Box<Node> {
    let Some(mut node) = node else {
        return Node::new(low, high, value);
    };

    // Go's bstInsert orders by Low, then High; ties (and equal keys) go right,
    // matching `compareIntervals(n, current) < 0` ⇒ left, else right.
    let go_left = low < node.interval.low || (low == node.interval.low && high < node.interval.high);
    if go_left {
        node.left = Some(insert_node(node.left.take(), low, high, value));
    } else {
        node.right = Some(insert_node(node.right.take(), low, high, value));
    }

    update_height_and_max(&mut node);
    rebalance(node)
}

fn collect_overlap(node: &Option<Box<Node>>, low: u32, high: u32, result: &mut Vec<Interval>) {
    let Some(node) = node else { return };

    // Prune: if maxHigh in this subtree is less than the query low, skip it.
    if node.max_high < low {
        return;
    }

    collect_overlap(&node.left, low, high, result);

    if node.interval.low <= high && node.interval.high >= low {
        result.push(node.interval);
    }

    // Prune right: if node's Low > high, no right child can overlap.
    if node.interval.low > high {
        return;
    }

    collect_overlap(&node.right, low, high, result);
}

// `is_empty` and `query_point` mirror the Go `interval.Tree` API surface and
// are covered by this module's unit tests, but the burndown `range_query` path
// only uses `query_overlap`. Allow the dead-code warning rather than dropping
// API parity with the Go package this module vendors.
#[allow(dead_code)]
impl Tree {
    /// Create a new empty interval tree. Mirrors Go `New`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of intervals in the tree. Mirrors Go `Len`.
    pub fn len(&self) -> usize {
        self.size
    }

    /// `true` if the tree contains no intervals.
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Add an interval `[low, high]` with `value`. Mirrors Go `Insert`.
    pub fn insert(&mut self, low: u32, high: u32, value: u32) {
        self.root = Some(insert_node(self.root.take(), low, high, value));
        self.size += 1;
    }

    /// Return all intervals overlapping `[low, high]`, in ascending `(low, high)`
    /// order. Mirrors Go `QueryOverlap`.
    pub fn query_overlap(&self, low: u32, high: u32) -> Vec<Interval> {
        let mut result = Vec::new();
        collect_overlap(&self.root, low, high, &mut result);
        result
    }

    /// Return all intervals containing `point`. Mirrors Go `QueryPoint`.
    pub fn query_point(&self, point: u32) -> Vec<Interval> {
        self.query_overlap(point, point)
    }

    /// Remove all intervals from the tree. Mirrors Go `Clear`.
    pub fn clear(&mut self) {
        self.root = None;
        self.size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of Go interval_test.go empty-tree behavior.
    #[test]
    fn empty_tree() {
        let t = Tree::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert!(t.query_overlap(0, 100).is_empty());
        assert!(t.query_point(5).is_empty());
    }

    /// Single interval overlap and non-overlap.
    #[test]
    fn single_interval_overlap() {
        let mut t = Tree::new();
        t.insert(10, 20, 1);
        assert_eq!(t.len(), 1);
        assert_eq!(t.query_overlap(15, 25), vec![Interval { low: 10, high: 20, value: 1 }]);
        assert!(t.query_overlap(21, 30).is_empty());
        assert!(t.query_overlap(0, 9).is_empty());
    }

    /// Overlap at the inclusive boundaries (`a <= high AND b >= low`).
    #[test]
    fn boundary_overlap() {
        let mut t = Tree::new();
        t.insert(10, 20, 1);
        // Touching at the low edge.
        assert_eq!(t.query_overlap(20, 30).len(), 1);
        // Touching at the high edge.
        assert_eq!(t.query_overlap(0, 10).len(), 1);
    }

    /// Multiple overlaps must come back ascending by `(low, high)` (in-order).
    #[test]
    fn multiple_overlaps_sorted() {
        let mut t = Tree::new();
        t.insert(30, 40, 3);
        t.insert(0, 10, 1);
        t.insert(15, 25, 2);
        let got = t.query_overlap(0, 100);
        let lows: Vec<u32> = got.iter().map(|i| i.low).collect();
        assert_eq!(lows, vec![0, 15, 30]);
    }

    /// `query_point` is `query_overlap(point, point)`.
    #[test]
    fn query_point_matches_overlap() {
        let mut t = Tree::new();
        t.insert(5, 15, 7);
        assert_eq!(t.query_point(10), vec![Interval { low: 5, high: 15, value: 7 }]);
        assert!(t.query_point(20).is_empty());
    }

    /// `clear` resets the tree.
    #[test]
    fn clear_resets() {
        let mut t = Tree::new();
        t.insert(1, 2, 9);
        t.clear();
        assert!(t.is_empty());
        assert!(t.query_overlap(1, 2).is_empty());
    }

    /// Stays balanced after many ascending inserts (height ~ log2 n, not n).
    #[test]
    fn balanced_after_many_inserts() {
        let mut t = Tree::new();
        for i in 0..1000u32 {
            t.insert(i, i, i);
        }
        assert_eq!(t.len(), 1000);
        let h = height(&t.root);
        assert!(h < 20, "tree not balanced: height {h}");
    }

    /// Equal-key intervals with different values all surface in a point query.
    #[test]
    fn duplicate_keys() {
        let mut t = Tree::new();
        t.insert(10, 20, 1);
        t.insert(10, 20, 2);
        t.insert(10, 20, 3);
        let got = t.query_point(15);
        assert_eq!(got.len(), 3);
        let values: std::collections::HashSet<u32> = got.iter().map(|i| i.value).collect();
        assert_eq!(values, [1, 2, 3].into_iter().collect());
    }
}

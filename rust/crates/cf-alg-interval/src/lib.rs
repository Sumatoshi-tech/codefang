//! Augmented interval tree for efficient range-overlap queries.
//!
//! This crate is a Rust port of the Go package `pkg/alg/interval`. It provides
//! an augmented interval tree supporting [`Tree::insert`], [`Tree::delete`],
//! [`Tree::query_overlap`], and [`Tree::query_point`] operations with
//! `O(log N)` insert/delete and `O(log N + k)` query time, where `k` is the
//! number of overlapping intervals.
//!
//! The tree is backed by a red-black tree where each node stores the maximum
//! right endpoint (`max_high`) in its subtree, enabling subtree pruning during
//! overlap queries.
//!
//! Intervals are **closed** ranges `[low, high]`. Two intervals `[a, b]` and
//! `[low, high]` overlap when `a <= high && b >= low`.
//!
//! # Relationship to the Go original
//!
//! Behaviour is reproduced exactly: ordering, balancing, query results and
//! `max_high` augmentation all match the Go implementation. This crate emits no
//! machine-format report bytes (it is a pure data structure consumed by the
//! burndown analyzer), so per the Rust rewrite design it does NOT depend on the
//! `cf-gojson`/`cf-goyaml` serialization crates.
//!
//! # Memory model
//!
//! The Go implementation uses pointer-linked nodes with parent pointers. To
//! reproduce the exact red-black balancing without `unsafe`, this port stores
//! nodes in an arena (`Vec<Node<..>>`) and links them by index. Deleted nodes
//! are recycled through a free list so the arena does not grow without bound
//! across long insert/delete sequences.
//!
//! # Example
//!
//! ```
//! use cf_alg_interval::Tree;
//!
//! let mut tree: Tree<u32, u32> = Tree::new();
//! tree.insert(10, 20, 1);
//! tree.insert(15, 25, 2);
//! tree.insert(30, 40, 3);
//!
//! let mut overlaps = tree.query_overlap(18, 22);
//! overlaps.sort_by_key(|iv| iv.value);
//! let values: Vec<u32> = overlaps.iter().map(|iv| iv.value).collect();
//! assert_eq!(values, vec![1, 2]);
//!
//! assert_eq!(tree.len(), 3);
//! assert!(tree.delete(15, 25, &2));
//! assert_eq!(tree.len(), 2);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt::Debug;

/// Endpoint type for interval bounds.
///
/// In the Go original this is the `Integer` type-set constraint
/// (`~int | … | ~uintptr`). In Rust we accept any totally-ordered, copyable
/// key; integer types satisfy this and are the intended usage, but the bound is
/// expressed structurally rather than as a closed set of integer types.
pub trait Endpoint: Copy + Ord + Debug {}

impl<T: Copy + Ord + Debug> Endpoint for T {}

/// Value type stored alongside an interval.
///
/// The Go original requires `comparable` because [`Tree::delete`] matches on
/// the exact `(low, high, value)` triple. In Rust we require [`PartialEq`] for
/// the same equality check and [`Clone`] so values can be returned by
/// [`Tree::query_overlap`] / [`Tree::query_point`].
pub trait Value: PartialEq + Clone + Debug {}

impl<T: PartialEq + Clone + Debug> Value for T {}

/// A closed range `[low, high]` with an associated value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interval<K, V> {
    /// Inclusive lower bound of the interval.
    pub low: K,
    /// Inclusive upper bound of the interval.
    pub high: K,
    /// Value associated with the interval.
    pub value: V,
}

/// Red-black node colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

/// Sentinel "nil" index used for absent links (mirrors Go's `nil` pointers).
const NIL: usize = usize::MAX;

/// Internal red-black tree node augmented with `max_high`.
#[derive(Debug, Clone)]
struct Node<K, V> {
    interval: Interval<K, V>,
    max_high: K,
    left: usize,
    right: usize,
    parent: usize,
    color: Color,
}

/// An augmented interval tree supporting overlap queries.
///
/// `K` is the endpoint type (typically an unsigned/signed integer) and `V` is
/// the value stored with each interval.
#[derive(Debug, Clone)]
pub struct Tree<K, V> {
    nodes: Vec<Node<K, V>>,
    free: Vec<usize>,
    root: usize,
    size: usize,
}

impl<K: Endpoint, V: Value> Default for Tree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Endpoint, V: Value> Tree<K, V> {
    /// Creates an empty interval tree.
    #[must_use]
    pub fn new() -> Self {
        Tree {
            nodes: Vec::new(),
            free: Vec::new(),
            root: NIL,
            size: 0,
        }
    }

    /// Returns the number of intervals in the tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns `true` if the tree contains no intervals.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    /// Removes all intervals from the tree.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.free.clear();
        self.root = NIL;
        self.size = 0;
    }

    // ---- arena helpers ----------------------------------------------------

    fn alloc(&mut self, node: Node<K, V>) -> usize {
        if let Some(idx) = self.free.pop() {
            self.nodes[idx] = node;
            idx
        } else {
            self.nodes.push(node);
            self.nodes.len() - 1
        }
    }

    fn dealloc(&mut self, idx: usize) {
        self.free.push(idx);
    }

    #[inline]
    fn color_of(&self, idx: usize) -> Color {
        if idx == NIL {
            Color::Black
        } else {
            self.nodes[idx].color
        }
    }

    // ---- public mutators --------------------------------------------------

    /// Adds an interval `[low, high]` with the given value to the tree.
    pub fn insert(&mut self, low: K, high: K, value: V) {
        let n = self.alloc(Node {
            interval: Interval { low, high, value },
            max_high: high,
            left: NIL,
            right: NIL,
            parent: NIL,
            color: Color::Red,
        });

        self.bst_insert(n);
        self.insert_fixup(n);
        self.size += 1;
    }

    /// Removes one interval matching `(low, high, value)` from the tree.
    ///
    /// Returns `true` if the interval was found and removed, `false` otherwise.
    pub fn delete(&mut self, low: K, high: K, value: &V) -> bool {
        let n = self.find_node(low, high, value);
        if n == NIL {
            return false;
        }
        self.delete_node(n);
        self.size -= 1;
        true
    }

    // ---- queries ----------------------------------------------------------

    /// Returns all intervals that overlap the query range `[low, high]`.
    ///
    /// An interval `[a, b]` overlaps `[low, high]` when `a <= high && b >= low`.
    #[must_use]
    pub fn query_overlap(&self, low: K, high: K) -> Vec<Interval<K, V>> {
        let mut results = Vec::new();
        if self.root != NIL {
            self.collect_overlap(self.root, low, high, &mut results);
        }
        results
    }

    /// Returns all intervals that contain the point `p`
    /// (i.e. `low <= p <= high`). Equivalent to `query_overlap(p, p)`.
    #[must_use]
    pub fn query_point(&self, p: K) -> Vec<Interval<K, V>> {
        self.query_overlap(p, p)
    }

    fn collect_overlap(
        &self,
        idx: usize,
        low: K,
        high: K,
        results: &mut Vec<Interval<K, V>>,
    ) {
        if idx == NIL {
            return;
        }
        let node = &self.nodes[idx];

        // Prune: if the largest endpoint in this subtree is below the query
        // low bound, nothing here can overlap.
        if node.max_high < low {
            return;
        }

        // Search left subtree first (in-order traversal keeps a stable shape).
        if node.left != NIL {
            self.collect_overlap(node.left, low, high, results);
        }

        // Check the current interval: [a, b] overlaps [low, high] iff
        // a <= high && b >= low.
        if node.interval.low <= high && node.interval.high >= low {
            results.push(node.interval.clone());
        }

        // Prune the right subtree: if this node's low is already past the query
        // high bound, every node to the right starts even later.
        if node.interval.low <= high && node.right != NIL {
            self.collect_overlap(node.right, low, high, results);
        }
    }

    // ---- BST insert + max_high maintenance --------------------------------

    fn bst_insert(&mut self, z: usize) {
        let mut y = NIL;
        let mut x = self.root;

        let (zlow, zhigh) = {
            let zi = &self.nodes[z].interval;
            (zi.low, zi.high)
        };

        while x != NIL {
            y = x;
            // Augment on the way down: every ancestor's max_high must cover z.
            if self.nodes[x].max_high < zhigh {
                self.nodes[x].max_high = zhigh;
            }
            // Order by the full (low, high) key — identical to Go's bstInsert,
            // which uses compareIntervals (low then high). This MUST match the
            // comparator used by find_exact, or lookups break for equal-low
            // nodes that differ in high.
            let xlow = self.nodes[x].interval.low;
            let xhigh = self.nodes[x].interval.high;
            if Self::compare_key(zlow, zhigh, xlow, xhigh) == std::cmp::Ordering::Less {
                x = self.nodes[x].left;
            } else {
                x = self.nodes[x].right;
            }
        }

        self.nodes[z].parent = y;
        if y == NIL {
            self.root = z;
        } else {
            let ylow = self.nodes[y].interval.low;
            let yhigh = self.nodes[y].interval.high;
            if Self::compare_key(zlow, zhigh, ylow, yhigh) == std::cmp::Ordering::Less {
                self.nodes[y].left = z;
            } else {
                self.nodes[y].right = z;
            }
        }
    }

    /// Recomputes `max_high` for a single node from its own interval and the
    /// `max_high` of its children.
    fn update_max_high(&mut self, idx: usize) {
        if idx == NIL {
            return;
        }
        let mut m = self.nodes[idx].interval.high;
        let l = self.nodes[idx].left;
        if l != NIL && self.nodes[l].max_high > m {
            m = self.nodes[l].max_high;
        }
        let r = self.nodes[idx].right;
        if r != NIL && self.nodes[r].max_high > m {
            m = self.nodes[r].max_high;
        }
        self.nodes[idx].max_high = m;
    }

    /// Walks from `idx` to the root recomputing `max_high` at each step.
    fn propagate_max_high(&mut self, mut idx: usize) {
        while idx != NIL {
            self.update_max_high(idx);
            idx = self.nodes[idx].parent;
        }
    }

    // ---- rotations --------------------------------------------------------

    fn left_rotate(&mut self, x: usize) {
        let y = self.nodes[x].right;
        // x.right = y.left
        let yl = self.nodes[y].left;
        self.nodes[x].right = yl;
        if yl != NIL {
            self.nodes[yl].parent = x;
        }
        // y.parent = x.parent
        let xp = self.nodes[x].parent;
        self.nodes[y].parent = xp;
        if xp == NIL {
            self.root = y;
        } else if x == self.nodes[xp].left {
            self.nodes[xp].left = y;
        } else {
            self.nodes[xp].right = y;
        }
        // y.left = x
        self.nodes[y].left = x;
        self.nodes[x].parent = y;

        // Fix augmentation bottom-up: x first, then y.
        self.update_max_high(x);
        self.update_max_high(y);
    }

    fn right_rotate(&mut self, x: usize) {
        let y = self.nodes[x].left;
        let yr = self.nodes[y].right;
        self.nodes[x].left = yr;
        if yr != NIL {
            self.nodes[yr].parent = x;
        }
        let xp = self.nodes[x].parent;
        self.nodes[y].parent = xp;
        if xp == NIL {
            self.root = y;
        } else if x == self.nodes[xp].right {
            self.nodes[xp].right = y;
        } else {
            self.nodes[xp].left = y;
        }
        self.nodes[y].right = x;
        self.nodes[x].parent = y;

        self.update_max_high(x);
        self.update_max_high(y);
    }

    // ---- insert fixup -----------------------------------------------------

    fn insert_fixup(&mut self, mut z: usize) {
        while self.color_of(self.nodes[z].parent) == Color::Red {
            let p = self.nodes[z].parent;
            let g = self.nodes[p].parent;
            if g == NIL {
                break;
            }
            if p == self.nodes[g].left {
                let y = self.nodes[g].right; // uncle
                if self.color_of(y) == Color::Red {
                    self.nodes[p].color = Color::Black;
                    self.nodes[y].color = Color::Black;
                    self.nodes[g].color = Color::Red;
                    z = g;
                } else {
                    if z == self.nodes[p].right {
                        z = p;
                        self.left_rotate(z);
                    }
                    let p2 = self.nodes[z].parent;
                    let g2 = self.nodes[p2].parent;
                    self.nodes[p2].color = Color::Black;
                    if g2 != NIL {
                        self.nodes[g2].color = Color::Red;
                        self.right_rotate(g2);
                    }
                }
            } else {
                let y = self.nodes[g].left; // uncle
                if self.color_of(y) == Color::Red {
                    self.nodes[p].color = Color::Black;
                    self.nodes[y].color = Color::Black;
                    self.nodes[g].color = Color::Red;
                    z = g;
                } else {
                    if z == self.nodes[p].left {
                        z = p;
                        self.right_rotate(z);
                    }
                    let p2 = self.nodes[z].parent;
                    let g2 = self.nodes[p2].parent;
                    self.nodes[p2].color = Color::Black;
                    if g2 != NIL {
                        self.nodes[g2].color = Color::Red;
                        self.left_rotate(g2);
                    }
                }
            }
        }
        self.nodes[self.root].color = Color::Black;
    }

    // ---- lookup -----------------------------------------------------------

    fn find_node(&self, low: K, high: K, value: &V) -> usize {
        self.find_exact(self.root, low, high, value)
    }

    /// Compares two `(low, high)` keys for BST ordering: primary by `low`,
    /// secondary by `high`. Mirrors Go's `compareIntervals`.
    #[inline]
    fn compare_key(alow: K, ahigh: K, blow: K, bhigh: K) -> std::cmp::Ordering {
        match alow.cmp(&blow) {
            std::cmp::Ordering::Equal => ahigh.cmp(&bhigh),
            other => other,
        }
    }

    /// Faithful port of Go's `findExact`: ordered search by `(low, high)`; when
    /// the key compares equal but the value differs, the left subtree is also
    /// checked (duplicate keys with different values can land on either side
    /// after red-black rotations), then the right subtree.
    fn find_exact(&self, n: usize, low: K, high: K, value: &V) -> usize {
        if n == NIL {
            return NIL;
        }

        let nlow = self.nodes[n].interval.low;
        let nhigh = self.nodes[n].interval.high;
        let cmp = Self::compare_key(low, high, nlow, nhigh);

        if cmp == std::cmp::Ordering::Equal && self.nodes[n].interval.value == *value {
            return n;
        }

        if cmp == std::cmp::Ordering::Less {
            return self.find_exact(self.nodes[n].left, low, high, value);
        }

        // cmp > 0, or cmp == 0 but value did not match: also check the left
        // subtree for an equal key, then search right (mirrors Go findExact).
        if cmp == std::cmp::Ordering::Equal {
            let found = self.find_exact(self.nodes[n].left, low, high, value);
            if found != NIL {
                return found;
            }
        }

        self.find_exact(self.nodes[n].right, low, high, value)
    }

    // ---- delete -----------------------------------------------------------

    #[inline]
    fn minimum(&self, mut x: usize) -> usize {
        while self.nodes[x].left != NIL {
            x = self.nodes[x].left;
        }
        x
    }

    /// Replaces subtree rooted at `u` with subtree rooted at `v`.
    fn transplant(&mut self, u: usize, v: usize) {
        let up = self.nodes[u].parent;
        if up == NIL {
            self.root = v;
        } else if u == self.nodes[up].left {
            self.nodes[up].left = v;
        } else {
            self.nodes[up].right = v;
        }
        if v != NIL {
            self.nodes[v].parent = up;
        }
    }

    fn delete_node(&mut self, z: usize) {
        let mut y = z;
        let mut y_original_color = self.nodes[y].color;
        let x;
        // x_parent tracks the parent for the case where x == NIL, so the
        // augmentation/fixup walk has a starting node.
        let x_parent;

        if self.nodes[z].left == NIL {
            x = self.nodes[z].right;
            x_parent = self.nodes[z].parent;
            self.transplant(z, self.nodes[z].right);
        } else if self.nodes[z].right == NIL {
            x = self.nodes[z].left;
            x_parent = self.nodes[z].parent;
            self.transplant(z, self.nodes[z].left);
        } else {
            y = self.minimum(self.nodes[z].right);
            y_original_color = self.nodes[y].color;
            x = self.nodes[y].right;
            if self.nodes[y].parent == z {
                x_parent = y;
                if x != NIL {
                    self.nodes[x].parent = y;
                }
            } else {
                x_parent = self.nodes[y].parent;
                let yr = self.nodes[y].right;
                self.transplant(y, yr);
                self.nodes[y].right = self.nodes[z].right;
                let yr2 = self.nodes[y].right;
                self.nodes[yr2].parent = y;
            }
            self.transplant(z, y);
            self.nodes[y].left = self.nodes[z].left;
            let yl = self.nodes[y].left;
            self.nodes[yl].parent = y;
            self.nodes[y].color = self.nodes[z].color;
        }

        // Recompute augmentation from the structural change point up to root.
        self.propagate_max_high(x_parent);

        if y_original_color == Color::Black {
            self.delete_fixup(x, x_parent);
        }

        self.dealloc(z);
    }

    /// Red-black delete rebalancing. `x` may be `NIL`; `x_parent` is its parent
    /// (needed because we cannot dereference a NIL sentinel for its parent).
    fn delete_fixup(&mut self, mut x: usize, mut x_parent: usize) {
        while x != self.root && self.color_of(x) == Color::Black {
            if x_parent == NIL {
                break;
            }
            if x == self.nodes[x_parent].left {
                let mut w = self.nodes[x_parent].right;
                if self.color_of(w) == Color::Red {
                    self.nodes[w].color = Color::Black;
                    self.nodes[x_parent].color = Color::Red;
                    self.left_rotate(x_parent);
                    w = self.nodes[x_parent].right;
                }
                if w == NIL {
                    x = x_parent;
                    x_parent = self.nodes[x].parent;
                    continue;
                }
                let wl = self.nodes[w].left;
                let wr = self.nodes[w].right;
                if self.color_of(wl) == Color::Black && self.color_of(wr) == Color::Black {
                    self.nodes[w].color = Color::Red;
                    x = x_parent;
                    x_parent = self.nodes[x].parent;
                } else {
                    if self.color_of(wr) == Color::Black {
                        if wl != NIL {
                            self.nodes[wl].color = Color::Black;
                        }
                        self.nodes[w].color = Color::Red;
                        self.right_rotate(w);
                        w = self.nodes[x_parent].right;
                    }
                    self.nodes[w].color = self.nodes[x_parent].color;
                    self.nodes[x_parent].color = Color::Black;
                    let wr2 = self.nodes[w].right;
                    if wr2 != NIL {
                        self.nodes[wr2].color = Color::Black;
                    }
                    self.left_rotate(x_parent);
                    x = self.root;
                    x_parent = NIL;
                }
            } else {
                let mut w = self.nodes[x_parent].left;
                if self.color_of(w) == Color::Red {
                    self.nodes[w].color = Color::Black;
                    self.nodes[x_parent].color = Color::Red;
                    self.right_rotate(x_parent);
                    w = self.nodes[x_parent].left;
                }
                if w == NIL {
                    x = x_parent;
                    x_parent = self.nodes[x].parent;
                    continue;
                }
                let wr = self.nodes[w].right;
                let wl = self.nodes[w].left;
                if self.color_of(wr) == Color::Black && self.color_of(wl) == Color::Black {
                    self.nodes[w].color = Color::Red;
                    x = x_parent;
                    x_parent = self.nodes[x].parent;
                } else {
                    if self.color_of(wl) == Color::Black {
                        if wr != NIL {
                            self.nodes[wr].color = Color::Black;
                        }
                        self.nodes[w].color = Color::Red;
                        self.left_rotate(w);
                        w = self.nodes[x_parent].left;
                    }
                    self.nodes[w].color = self.nodes[x_parent].color;
                    self.nodes[x_parent].color = Color::Black;
                    let wl2 = self.nodes[w].left;
                    if wl2 != NIL {
                        self.nodes[wl2].color = Color::Black;
                    }
                    self.right_rotate(x_parent);
                    x = self.root;
                    x_parent = NIL;
                }
            }
        }
        if x != NIL {
            self.nodes[x].color = Color::Black;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted_values(mut ivs: Vec<Interval<u32, u32>>) -> Vec<u32> {
        ivs.sort_by(|a, b| a.value.cmp(&b.value));
        ivs.into_iter().map(|iv| iv.value).collect()
    }

    fn sorted_values_i64<V: Ord + Clone>(mut ivs: Vec<Interval<i64, V>>) -> Vec<V> {
        ivs.sort_by(|a, b| a.value.cmp(&b.value));
        ivs.into_iter().map(|iv| iv.value).collect()
    }

    // ---- ported from Go interval_test.go ---------------------------------

    /// Port of `TestNew`.
    #[test]
    fn new_tree_is_empty() {
        let tree: Tree<u32, u32> = Tree::new();
        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
    }

    /// Port of `TestInsert_Len`.
    #[test]
    fn insert_len() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        assert_eq!(tree.len(), 1);
        tree.insert(30, 40, 2);
        assert_eq!(tree.len(), 2);
    }

    /// Port of `TestInsert_QueryOverlap_Basic`.
    #[test]
    fn insert_query_overlap_basic() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        let results = tree.query_overlap(15, 25);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].low, 10);
        assert_eq!(results[0].high, 20);
        assert_eq!(results[0].value, 1);
    }

    /// Port of `TestQueryOverlap_NoMatch`.
    #[test]
    fn query_overlap_no_match() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        assert!(tree.query_overlap(30, 40).is_empty());
    }

    /// Port of `TestQueryOverlap_EmptyTree`.
    #[test]
    fn query_overlap_empty_tree() {
        let tree: Tree<u32, u32> = Tree::new();
        assert!(tree.query_overlap(10, 20).is_empty());
    }

    /// Port of `TestQueryOverlap_MultipleResults`.
    #[test]
    fn query_overlap_multiple_results() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.insert(15, 25, 2);
        tree.insert(30, 40, 3);
        // Query [12, 18] overlaps [10,20] and [15,25] but not [30,40].
        assert_eq!(tree.query_overlap(12, 18).len(), 2);
    }

    /// Port of `TestQueryPoint_Basic`.
    #[test]
    fn query_point_basic() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.insert(30, 40, 2);
        let results = tree.query_point(12);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].value, 1);
    }

    /// Port of `TestQueryPoint_Boundary`.
    #[test]
    fn query_point_boundary() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        // Low boundary inclusive.
        assert_eq!(tree.query_point(10).len(), 1);
        // High boundary inclusive.
        assert_eq!(tree.query_point(20).len(), 1);
    }

    /// Port of `TestQueryPoint_NoMatch`.
    #[test]
    fn query_point_no_match() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        assert!(tree.query_point(50).is_empty());
    }

    /// Port of `TestDelete_Basic`.
    #[test]
    fn delete_basic() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        assert!(tree.delete(10, 20, &1));
        assert_eq!(tree.len(), 0);
        assert!(tree.query_overlap(10, 20).is_empty());
    }

    /// Port of `TestDelete_NonExistent`.
    #[test]
    fn delete_non_existent() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        assert!(!tree.delete(30, 40, &2));
        assert_eq!(tree.len(), 1);
    }

    /// Port of `TestDelete_EmptyTree`.
    #[test]
    fn delete_empty_tree() {
        let mut tree: Tree<u32, u32> = Tree::new();
        assert!(!tree.delete(10, 20, &1));
    }

    /// Port of `TestDelete_PreservesOthers`.
    #[test]
    fn delete_preserves_others() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.insert(30, 40, 2);
        tree.delete(10, 20, &1);
        let results = tree.query_overlap(30, 40);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].value, 2);
    }

    /// Port of `TestClear`.
    #[test]
    fn clear() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.insert(30, 40, 2);
        assert_eq!(tree.len(), 2);
        tree.clear();
        assert_eq!(tree.len(), 0);
        assert!(tree.query_overlap(0, 100).is_empty());
        // Reusable after clear.
        tree.insert(5, 15, 7);
        assert_eq!(tree.len(), 1);
        assert_eq!(sorted_values(tree.query_point(10)), vec![7]);
    }

    /// Port of `TestAdjacentNonOverlapping`.
    #[test]
    fn adjacent_non_overlapping() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.insert(21, 40, 2);
        // At 20 -> first only.
        let r = tree.query_point(20);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, 1);
        // At 21 -> second only.
        let r = tree.query_point(21);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, 2);
    }

    /// Port of `TestZeroWidthInterval`.
    #[test]
    fn zero_width_interval() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(15, 15, 1);
        assert_eq!(tree.query_point(15).len(), 1);
        assert!(tree.query_point(10).is_empty());
    }

    /// Port of `TestLargeScale` (10K intervals).
    #[test]
    fn large_scale() {
        let mut tree: Tree<u32, u32> = Tree::new();
        const COUNT: u32 = 10_000;
        const WIDTH: u32 = 5;
        const SPACING: u32 = 10;
        for i in 0..COUNT {
            let low = i * SPACING;
            tree.insert(low, low + WIDTH, i);
        }
        assert_eq!(tree.len(), COUNT as usize);

        // [0,5],[10,15],...,[990,995] all have low < 1000 -> 100 intervals.
        assert_eq!(tree.query_overlap(0, 995).len(), 100);

        // Point at 50*10 = 500 -> exactly interval value 50 ([500,505]).
        let r = tree.query_point(500);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, 50);
    }

    /// Port of `TestDeleteMultiple`.
    #[test]
    fn delete_multiple() {
        let mut tree: Tree<u32, u32> = Tree::new();
        const COUNT: u32 = 20;
        for i in 0..COUNT {
            tree.insert(i * 10, i * 10 + 5, i);
        }
        assert_eq!(tree.len(), COUNT as usize);
        for i in 0..COUNT {
            assert!(tree.delete(i * 10, i * 10 + 5, &i), "delete failed at index {i}");
        }
        assert_eq!(tree.len(), 0);
    }

    /// Port of `TestInsertDuplicateIntervals`.
    #[test]
    fn insert_duplicate_intervals() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.insert(10, 20, 1);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.query_overlap(10, 20).len(), 2);
        // Delete one -> one remains.
        tree.delete(10, 20, &1);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.query_overlap(10, 20).len(), 1);
    }

    /// Port of `TestWideOverlap`.
    #[test]
    fn wide_overlap() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.insert(15, 25, 2);
        tree.insert(30, 40, 3);
        tree.insert(5, 35, 4);
        assert_eq!(tree.query_overlap(0, 50).len(), 4);
    }

    /// Port of `TestDeleteAndReinsert`.
    #[test]
    fn delete_and_reinsert() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 20, 1);
        tree.delete(10, 20, &1);
        assert_eq!(tree.len(), 0);
        tree.insert(10, 20, 2);
        assert_eq!(tree.len(), 1);
        let r = tree.query_overlap(10, 20);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, 2);
    }

    /// Port of `TestGeneric_IntKeys` (int keys, string values).
    #[test]
    fn generic_int_keys() {
        let mut tree: Tree<i32, String> = Tree::new();
        tree.insert(100, 200, "alpha".to_string());
        tree.insert(150, 250, "beta".to_string());
        tree.insert(300, 400, "gamma".to_string());
        assert_eq!(tree.len(), 3);

        // Point 175 -> [100,200] and [150,250].
        assert_eq!(tree.query_point(175).len(), 2);

        // [300,400] -> only gamma.
        let r = tree.query_overlap(300, 400);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, "gamma");

        // Delete alpha.
        assert!(tree.delete(100, 200, &"alpha".to_string()));
        assert_eq!(tree.len(), 2);
        let mut r = tree.query_point(175);
        assert_eq!(r.len(), 1);
        r.sort_by(|a, b| a.value.cmp(&b.value));
        assert_eq!(r[0].value, "beta");
    }

    /// Port of `TestGeneric_Int64Keys`.
    #[test]
    fn generic_int64_keys() {
        let mut tree: Tree<i64, i64> = Tree::new();
        tree.insert(1_000_000_000, 2_000_000_000, 1);
        tree.insert(1_500_000_000, 2_500_000_000, 2);
        tree.insert(3_000_000_000, 4_000_000_000, 3);
        assert_eq!(tree.len(), 3);

        // 1.75B -> [1B,2B] and [1.5B,2.5B].
        assert_eq!(tree.query_point(1_750_000_000).len(), 2);

        // Non-overlapping query.
        assert!(tree
            .query_overlap(4_000_000_001, 4_000_000_000 + 1_000_000_000)
            .is_empty());

        // Delete value 2.
        assert!(tree.delete(1_500_000_000, 2_500_000_000, &2));
        assert_eq!(tree.len(), 2);
        let r = tree.query_point(1_750_000_000);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].value, 1);

        tree.clear();
        assert_eq!(tree.len(), 0);
    }

    /// Behavioural port of `TestMaxHighMaintenance`. The Go test reaches into
    /// the private `tree.root.maxHigh` field directly; the Rust API does not
    /// expose internal node state, so we assert the *observable* consequence of
    /// correct `max_high` maintenance instead: pruning must not drop a still
    /// reachable interval after the widest one is deleted.
    #[test]
    fn max_high_maintenance() {
        let mut tree: Tree<u32, u32> = Tree::new();
        tree.insert(10, 60, 1);
        tree.insert(30, 40, 2);

        // Both intervals are reachable while [10,60] dominates max_high.
        assert_eq!(sorted_values(tree.query_point(35)), vec![1, 2]);

        // Deleting the wide interval must shrink max_high so the remaining
        // [30,40] is still found and a point only covered by [10,60] is not.
        assert!(tree.delete(10, 60, &1));
        assert_eq!(sorted_values(tree.query_point(35)), vec![2]);
        assert!(tree.query_point(55).is_empty());
    }

    /// Covers signed endpoints crossing zero (extends the Go int64 coverage).
    #[test]
    fn signed_endpoints() {
        let mut tree: Tree<i64, u32> = Tree::new();
        tree.insert(-100, -50, 1);
        tree.insert(-30, 30, 2);
        tree.insert(50, 100, 3);
        assert_eq!(sorted_values_i64(tree.query_point(0)), vec![2]);
        assert_eq!(sorted_values_i64(tree.query_point(-75)), vec![1]);
        assert_eq!(sorted_values_i64(tree.query_overlap(-200, 200)), vec![1, 2, 3]);
        assert!(tree.query_point(40).is_empty());
    }

    /// Stress test cross-checked against a naive linear reference. Mirrors the
    /// correctness intent of the Go benchmarks plus large-scale tests.
    #[test]
    fn stress_against_naive_reference() {
        let mut tree: Tree<u32, u32> = Tree::new();
        let mut reference: Vec<Interval<u32, u32>> = Vec::new();

        // Deterministic LCG so the test is reproducible without an rng dep.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u32
        };

        for i in 0..500u32 {
            let low = next() % 1000;
            let high = low + next() % 50;
            tree.insert(low, high, i);
            reference.push(Interval { low, high, value: i });
        }
        assert_eq!(tree.len(), reference.len());

        for _ in 0..200 {
            let qlow = next() % 1000;
            let qhigh = qlow + next() % 100;
            let mut got = sorted_values(tree.query_overlap(qlow, qhigh));
            let mut want: Vec<u32> = reference
                .iter()
                .filter(|iv| iv.low <= qhigh && iv.high >= qlow)
                .map(|iv| iv.value)
                .collect();
            got.sort_unstable();
            want.sort_unstable();
            assert_eq!(got, want, "query [{qlow},{qhigh}] mismatch");
        }

        // Delete the even-valued half.
        let to_delete: Vec<Interval<u32, u32>> =
            reference.iter().filter(|iv| iv.value % 2 == 0).cloned().collect();
        for iv in &to_delete {
            assert!(tree.delete(iv.low, iv.high, &iv.value));
        }
        reference.retain(|iv| iv.value % 2 == 1);
        assert_eq!(tree.len(), reference.len());

        for _ in 0..200 {
            let qlow = next() % 1000;
            let qhigh = qlow + next() % 100;
            let mut got = sorted_values(tree.query_overlap(qlow, qhigh));
            let mut want: Vec<u32> = reference
                .iter()
                .filter(|iv| iv.low <= qhigh && iv.high >= qlow)
                .map(|iv| iv.value)
                .collect();
            got.sort_unstable();
            want.sort_unstable();
            assert_eq!(got, want, "post-delete query [{qlow},{qhigh}] mismatch");
        }
    }
}

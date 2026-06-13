//! K-gram shingling of UAST function subtrees.
//!
//! A *shingle* is a sequence of `k` consecutive node types from a pre-order
//! traversal, joined by `"|"`. These shingles are the set elements that feed
//! each function's `MinHash` signature.

use cf_uast_node::Node;

/// Default k-gram window size for shingling.
pub const DEFAULT_SHINGLE_SIZE: usize = 5;

/// Separator placed between node types within a shingle.
pub const SHINGLE_SEPARATOR: &str = "|";

/// Extracts k-gram shingles from a function's UAST subtree.
#[derive(Debug, Clone, Copy)]
pub struct Shingler {
    k: usize,
}

impl Default for Shingler {
    fn default() -> Self {
        Self::new(DEFAULT_SHINGLE_SIZE)
    }
}

impl Shingler {
    /// Creates a new shingler with the given k-gram size.
    #[must_use]
    pub fn new(k: usize) -> Self {
        Self { k }
    }

    /// The configured k-gram window size.
    #[must_use]
    pub fn k(&self) -> usize {
        self.k
    }

    /// Returns the k-gram shingles of `func_node`'s subtree.
    ///
    /// Each shingle is the UTF-8 bytes of `k` consecutive node types joined by
    /// [`SHINGLE_SEPARATOR`]. Returns an empty vector if the subtree has fewer
    /// than `k` typed nodes.
    ///
    /// ```
    /// use cf_clones::Shingler;
    /// use cf_uast_node::Builder;
    ///
    /// // A left-deep chain A -> B -> C so pre-order yields [A, B, C].
    /// let c = Builder::new().with_type("C").build();
    /// let mut b = Builder::new().with_type("B").build();
    /// b.add_child(c);
    /// let mut a = Builder::new().with_type("A").build();
    /// a.add_child(b);
    ///
    /// let shingles = Shingler::new(2).extract_shingles(&a);
    /// let strings: Vec<String> = shingles
    ///     .iter()
    ///     .map(|s| String::from_utf8(s.clone()).unwrap())
    ///     .collect();
    /// assert_eq!(strings, vec!["A|B".to_string(), "B|C".to_string()]);
    ///
    /// // Fewer than k typed nodes -> no shingles.
    /// assert!(Shingler::new(5).extract_shingles(&a).is_empty());
    /// ```
    #[must_use]
    pub fn extract_shingles(&self, func_node: &Node) -> Vec<Vec<u8>> {
        let types = collect_node_types(func_node);
        if types.len() < self.k {
            return Vec::new();
        }

        let shingle_count = types.len() - self.k + 1;
        let mut shingles = Vec::with_capacity(shingle_count);
        for i in 0..shingle_count {
            shingles.push(join_types(&types[i..i + self.k]).into_bytes());
        }
        shingles
    }
}

/// Collects node types in pre-order, skipping empty-typed nodes.
#[must_use]
pub fn collect_node_types(root: &Node) -> Vec<String> {
    let mut types = Vec::new();
    root.visit_pre_order(|n: &Node| {
        if !n.node_type.is_empty() {
            types.push(n.node_type.clone());
        }
    });
    types
}

/// Joins node-type strings with [`SHINGLE_SEPARATOR`].
#[must_use]
fn join_types(types: &[String]) -> String {
    types.join(SHINGLE_SEPARATOR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uast::NodeBuilder;
    use cf_uast_node::Node;

    fn chain(types: &[&str]) -> Node {
        // Build a left-deep chain so pre-order yields the types in order.
        let mut node = NodeBuilder::new(types[types.len() - 1]).build();
        for t in types[..types.len() - 1].iter().rev() {
            let parent = NodeBuilder::new(t).child(node).build();
            node = parent;
        }
        node
    }

    #[test]
    fn collect_node_types_preorder() {
        let tree = chain(&["A", "B", "C"]);
        assert_eq!(collect_node_types(&tree), vec!["A", "B", "C"]);
    }

    #[test]
    fn collect_skips_empty_types() {
        // A node with empty type must not contribute.
        let inner = NodeBuilder::new("B").build();
        let mut mid = NodeBuilder::new("").build(); // empty type
        mid.add_child(inner);
        let root = NodeBuilder::new("A").child(mid).build();
        assert_eq!(collect_node_types(&root), vec!["A", "B"]);
    }

    #[test]
    fn fewer_than_k_types_yields_no_shingles() {
        let s = Shingler::new(5);
        let tree = chain(&["A", "B", "C"]);
        assert!(s.extract_shingles(&tree).is_empty());
    }

    #[test]
    fn shingles_are_k_grams_joined_by_pipe() {
        let s = Shingler::new(2);
        let tree = chain(&["A", "B", "C"]);
        let shingles = s.extract_shingles(&tree);
        let as_strings: Vec<String> = shingles
            .iter()
            .map(|b| String::from_utf8(b.clone()).unwrap())
            .collect();
        assert_eq!(as_strings, vec!["A|B".to_string(), "B|C".to_string()]);
    }

    #[test]
    fn default_k_is_five() {
        assert_eq!(Shingler::default().k(), DEFAULT_SHINGLE_SIZE);
        assert_eq!(DEFAULT_SHINGLE_SIZE, 5);
    }
}

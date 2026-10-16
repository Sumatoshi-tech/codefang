//! Structural profile of one function subtree.
//!
//! A [`Profile`] carries everything the clone engine needs to know about a
//! function: its size, the pre-order node-type sequence (the shingle input),
//! and two fingerprints — one over the structure (types only) and one over
//! the leaf tokens — that decide between Type-1 and Type-2 for structurally
//! identical pairs. Comment nodes are skipped entirely, so comment-only
//! differences never separate two clones.

use std::hash::{DefaultHasher, Hasher};

use cf_uast_node::Node;

/// FNV-1a 64-bit offset basis.
const FNV64A_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV64A_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Byte placed between the fields fed to a fingerprint so `("ab", "c")` and
/// `("a", "bc")` hash differently. Node types and tokens never contain it.
const FIELD_SEPARATOR: &[u8] = &[0];

/// Node type of comment nodes (ignored by the profile).
const COMMENT_TYPE: &str = "Comment";
/// Role of comment nodes (ignored by the profile).
const COMMENT_ROLE: &str = "Comment";

/// A 128-bit content fingerprint: FNV-1a 64 and `SipHash` (std's
/// [`DefaultHasher`]) over the same byte stream. Two independent hash
/// families make an accidental collision between distinct inputs
/// negligible; the values never leave the process, so neither needs to be
/// stable across Rust releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    fnv: u64,
    sip: u64,
}

/// Accumulates both fingerprint hashes over a byte stream.
struct DualHasher {
    fnv: u64,
    sip: DefaultHasher,
}

impl DualHasher {
    fn new() -> Self {
        Self {
            fnv: FNV64A_OFFSET,
            sip: DefaultHasher::new(),
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.fnv ^= u64::from(b);
            self.fnv = self.fnv.wrapping_mul(FNV64A_PRIME);
        }
        self.sip.write(bytes);
    }

    fn finish(self) -> Fingerprint {
        Fingerprint {
            fnv: self.fnv,
            sip: self.sip.finish(),
        }
    }
}

/// What the clone engine knows about one function subtree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile<'a> {
    /// Number of nodes in the subtree, comments excluded.
    pub node_count: usize,
    /// Pre-order node types (empty-typed and comment nodes skipped).
    pub types: Vec<&'a str>,
    /// Fingerprint of the node-type sequence: equal structure ⇔ equal
    /// fingerprint.
    pub structure: Fingerprint,
    /// Fingerprint of the leaf `(type, token)` sequence: equal structure and
    /// equal tokens ⇔ equal `structure` and equal `tokens`.
    pub tokens: Fingerprint,
}

/// Returns `true` for comment nodes, which the profile ignores.
fn is_comment(n: &Node) -> bool {
    n.node_type == COMMENT_TYPE || n.has_any_role(&[COMMENT_ROLE])
}

/// Builds the [`Profile`] of the subtree rooted at `root` with an explicit
/// stack (no recursion, so arbitrarily deep trees are safe).
#[must_use]
pub fn profile(root: &Node) -> Profile<'_> {
    let mut node_count = 0;
    let mut types = Vec::new();
    let mut structure = DualHasher::new();
    let mut tokens = DualHasher::new();

    let mut stack: Vec<&Node> = vec![root];
    while let Some(n) = stack.pop() {
        if is_comment(n) {
            continue;
        }
        node_count += 1;
        if !n.node_type.is_empty() {
            types.push(n.node_type.as_str());
            structure.write(n.node_type.as_bytes());
            structure.write(FIELD_SEPARATOR);
        }
        if n.children.is_empty() {
            tokens.write(n.node_type.as_bytes());
            tokens.write(FIELD_SEPARATOR);
            tokens.write(n.token.as_bytes());
            tokens.write(FIELD_SEPARATOR);
        }
        for child in n.children.iter().rev() {
            stack.push(child);
        }
    }

    Profile {
        node_count,
        types,
        structure: structure.finish(),
        tokens: tokens.finish(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast_node::Builder;

    fn node(node_type: &str, token: &str, children: Vec<Node>) -> Node {
        let mut n = Builder::new()
            .with_type(node_type)
            .with_token(token)
            .build();
        for c in children {
            n.add_child(c);
        }
        n
    }

    fn leaf(node_type: &str, token: &str) -> Node {
        node(node_type, token, Vec::new())
    }

    #[test]
    fn preorder_types_and_count() {
        let tree = node(
            "A",
            "",
            vec![leaf("B", "b"), node("C", "", vec![leaf("D", "d")])],
        );
        let p = profile(&tree);
        assert_eq!(p.node_count, 4);
        assert_eq!(p.types, vec!["A", "B", "C", "D"]);
    }

    #[test]
    fn empty_types_are_skipped_but_counted() {
        let tree = node("A", "", vec![node("", "", vec![leaf("B", "")])]);
        let p = profile(&tree);
        assert_eq!(p.node_count, 3);
        assert_eq!(p.types, vec!["A", "B"]);
    }

    #[test]
    fn comments_are_ignored_entirely() {
        let plain = node("A", "", vec![leaf("B", "b")]);
        let commented = node("A", "", vec![leaf("Comment", "// hi"), leaf("B", "b")]);
        let p = profile(&plain);
        let q = profile(&commented);
        assert_eq!(p, q);
        assert_eq!(q.node_count, 2);
    }

    #[test]
    fn renamed_tokens_keep_structure_but_change_tokens() {
        let a = node("A", "", vec![leaf("Identifier", "x"), leaf("Literal", "1")]);
        let b = node("A", "", vec![leaf("Identifier", "y"), leaf("Literal", "1")]);
        let (p, q) = (profile(&a), profile(&b));
        assert_eq!(p.structure, q.structure);
        assert_ne!(p.tokens, q.tokens);
    }

    #[test]
    fn different_structure_changes_the_structure_fingerprint() {
        let a = node("A", "", vec![leaf("Identifier", "x")]);
        let b = node("A", "", vec![leaf("Identifier", "x"), leaf("Literal", "1")]);
        assert_ne!(profile(&a).structure, profile(&b).structure);
    }

    #[test]
    fn internal_tokens_do_not_affect_the_token_fingerprint() {
        // Only LEAF tokens count, so whitespace/formatting captured in an
        // internal node's full-text token never separates two clones.
        let a = node("Block", "{ x }", vec![leaf("Identifier", "x")]);
        let b = node("Block", "{\n\tx\n}", vec![leaf("Identifier", "x")]);
        assert_eq!(profile(&a).tokens, profile(&b).tokens);
    }

    #[test]
    fn field_boundaries_are_separated() {
        let a = node(
            "A",
            "",
            vec![leaf("Identifier", "ab"), leaf("Identifier", "c")],
        );
        let b = node(
            "A",
            "",
            vec![leaf("Identifier", "a"), leaf("Identifier", "bc")],
        );
        assert_ne!(profile(&a).tokens, profile(&b).tokens);
    }
}

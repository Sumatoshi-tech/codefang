//! The `MinHash` + LSH clone-detection core.
//!
//! Signature building and pair finding live in one module because they form a
//! single detection pipeline shared by the [`crate::Analyzer`] (per-file) and
//! the [`crate::Aggregator`] (cross-file).

use std::collections::{HashMap, HashSet};

use cf_alg_lsh::Index;
use cf_alg_minhash::Signature;
use cf_uast_node::Node;

use crate::report::{classify_clone_type, ClonePair, CloneTypeCounts};
use crate::shingler::Shingler;
use crate::uast::{ROLE_DECLARATION, ROLE_FUNCTION, ROLE_PARAMETER, UAST_FUNCTION, UAST_METHOD};
use crate::{MIN_FUNCTION_NODES, NUM_HASHES};

/// A function's name plus its `MinHash` signature.
#[derive(Debug, Clone)]
pub struct FuncEntry {
    /// The (possibly receiver-qualified) function name.
    pub name: String,
    /// The function's `MinHash` signature.
    pub sig: Signature,
}

/// Output of [`find_clone_pairs`].
#[derive(Default)]
pub struct ClonePairResult {
    /// The stored clone pairs (may be capped; see [`find_clone_pairs`]).
    pub pairs: Vec<ClonePair>,
    /// The total count of unique pairs found, regardless of the cap.
    pub total_count: usize,
    /// Distribution of clone types across all pairs found.
    pub type_distribution: CloneTypeCounts,
    /// Distinct function names involved in any pair.
    pub cloned_func: HashSet<String>,
}

/// Returns `true` if `n` represents a function.
#[must_use]
pub fn is_function_node(n: &Node) -> bool {
    n.has_any_type(&[UAST_FUNCTION, UAST_METHOD])
        || n.has_all_roles(&[ROLE_FUNCTION, ROLE_DECLARATION])
}

/// Returns the total number of nodes in a subtree.
#[must_use]
pub fn count_nodes(n: &Node) -> usize {
    let mut count = 1;
    for child in &n.children {
        count += count_nodes(child);
    }
    count
}

/// Extracts a unique function name from a node.
///
/// Uses the entity name (see [`extract_entity_name`], falling back to the node
/// token, then the type); for methods, qualifies with the receiver type (e.g.
/// `"Foo.DoWork"`).
#[must_use]
pub fn extract_func_name(fn_node: &Node) -> String {
    let mut name = extract_entity_name(fn_node).unwrap_or_default();
    if name.is_empty() {
        name = if fn_node.token.is_empty() {
            fn_node.node_type.clone()
        } else {
            fn_node.token.clone()
        };
    }

    if fn_node.node_type == UAST_METHOD {
        let recv = extract_receiver_type(fn_node);
        if !recv.is_empty() {
            return format!("{recv}.{name}");
        }
    }

    name
}

/// Extracts the entity (function/identifier) name from a node.
///
/// Precedence order (report contract; pinned by the differential gate):
///  1. the node's `name` prop;
///  2. the node's OWN token;
///  3. the **first child** (index 0): its token, then its `name` prop.
///
/// The node's own token is checked BEFORE descending to a child, and the child
/// step looks only at index 0 (not "the first child with the Name role"). Both
/// points matter: e.g. C `Function` nodes whose token is the full function
/// text must keep that text as the name so distinct functions stay distinct in
/// the LSH index. Returns `None` when nothing usable is found; a present-but-
/// empty `name` prop yields `Some("")`, which `extract_func_name` treats
/// identically to `None`, so the observable name is the same.
#[must_use]
fn extract_entity_name(n: &Node) -> Option<String> {
    // 1. the "name" prop — present-key wins even if empty.
    if let Some(name) = n.props.get("name") {
        return Some(name.clone());
    }

    // 2. node's own token.
    if !n.token.is_empty() {
        return Some(n.token.clone());
    }

    // 3. first child (index 0): token, then its "name" prop.
    if let Some(child) = n.children.first() {
        if !child.token.is_empty() {
            return Some(child.token.clone());
        }
        if let Some(name) = child.props.get("name") {
            return Some(name.clone());
        }
    }

    None
}

/// Extracts the receiver type name from a method node.
///
/// The receiver is the first child with the `Parameter` role whose token looks
/// like `"(v *T)"` / `"(v T)"`; the type `T` is the last whitespace-separated
/// token after stripping the surrounding parens and a leading `*`.
#[must_use]
pub fn extract_receiver_type(fn_node: &Node) -> String {
    const MIN_RECEIVER_PARTS: usize = 2;

    for child in &fn_node.children {
        if !child.has_any_role(&[ROLE_PARAMETER]) {
            continue;
        }

        let tok = child.token.trim();
        if tok.is_empty() {
            continue;
        }

        let tok = tok
            .strip_prefix('(')
            .unwrap_or(tok)
            .strip_suffix(')')
            .unwrap_or_else(|| tok.strip_prefix('(').unwrap_or(tok))
            .trim();

        let parts: Vec<&str> = tok.split_whitespace().collect();
        if parts.len() < MIN_RECEIVER_PARTS {
            continue;
        }

        let type_name = parts[parts.len() - 1]
            .strip_prefix('*')
            .unwrap_or(parts[parts.len() - 1]);
        if !type_name.is_empty() {
            return type_name.to_string();
        }
    }

    String::new()
}

/// Builds a `MinHash` signature for one function subtree, or `None` when the
/// function is too small or produces no shingles.
#[must_use]
#[allow(clippy::similar_names)] // `shingler` / `shingles` is the clearest naming
pub fn build_signature(
    fn_node: &Node,
    shingler: &Shingler,
    num_hashes: usize,
) -> Option<FuncEntry> {
    if count_nodes(fn_node) < MIN_FUNCTION_NODES {
        return None;
    }

    let shingles = shingler.extract_shingles(fn_node);
    if shingles.is_empty() {
        return None;
    }

    let mut sig = Signature::new(num_hashes).ok()?;
    for shingle in &shingles {
        sig.add(shingle);
    }

    Some(FuncEntry {
        name: extract_func_name(fn_node),
        sig,
    })
}

/// Builds signatures for all functions, using the default [`NUM_HASHES`].
///
/// Convenience wrapper over [`build_signature`].
#[must_use]
pub fn build_signatures(
    functions: &[&Node],
    shingler: &Shingler,
    num_hashes: usize,
) -> Vec<FuncEntry> {
    let mut entries = Vec::with_capacity(functions.len());
    for fn_node in functions {
        if let Some(entry) = build_signature(fn_node, shingler, num_hashes) {
            entries.push(entry);
        }
    }
    entries
}

/// Returns a canonical key for a clone pair so `(A,B)` and `(B,A)` collide:
/// the two names are ordered lexicographically.
#[must_use]
fn clone_pair_key(func_a: &str, func_b: &str) -> (String, String) {
    if func_a > func_b {
        (func_b.to_string(), func_a.to_string())
    } else {
        (func_a.to_string(), func_b.to_string())
    }
}

/// Computes a clone pair between an entry and a candidate, or `None` if the
/// candidate is unknown or below `min_similarity`.
#[must_use]
fn compute_clone_pair(
    entry: &FuncEntry,
    candidate_id: &str,
    sig_map: &HashMap<String, Signature>,
    min_similarity: f64,
) -> Option<ClonePair> {
    let candidate_sig = sig_map.get(candidate_id)?;
    let similarity = entry.sig.similarity(candidate_sig).ok()?;
    if similarity < min_similarity {
        return None;
    }

    Some(ClonePair {
        func_a: entry.name.clone(),
        func_b: candidate_id.to_string(),
        similarity,
        clone_type: classify_clone_type(similarity).to_string(),
    })
}

/// Builds a name → signature lookup.
#[must_use]
fn build_signature_map(entries: &[FuncEntry]) -> HashMap<String, Signature> {
    let mut sig_map = HashMap::with_capacity(entries.len());
    for entry in entries {
        sig_map.insert(entry.name.clone(), entry.sig.clone());
    }
    sig_map
}

/// Queries the LSH index for every entry, collects unique clone pairs, and sorts
/// the stored pairs by similarity descending.
///
/// `pair_cap` limits the *stored* `pairs` slice (`0` = unlimited);
/// `total_count` always reflects all unique pairs found.
#[must_use]
pub fn find_clone_pairs(
    entries: &[FuncEntry],
    idx: &Index,
    pair_cap: usize,
    min_similarity: f64,
) -> ClonePairResult {
    let sig_map = build_signature_map(entries);
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut result = ClonePairResult::default();

    for entry in entries {
        let Ok(mut candidates) = idx.query_threshold(&entry.sig, min_similarity) else {
            continue;
        };

        // The LSH query collects candidates by iterating a `HashMap`, whose
        // order is randomized per run (the reference implementation is equally
        // order-nondeterministic here). That randomness propagates into the
        // discovery order, which (a) decides which pairs survive the
        // `pair_cap`, and (b) is the tie-break order among equal-similarity
        // pairs after the final sort. Sorting the candidates here makes
        // discovery order deterministic, so the stored pair SET and ORDER are
        // reproducible run-to-run (the compat harness canonicalizes the
        // reference's nondeterminism away, but requires our determinism).
        candidates.sort_unstable();

        for candidate_id in candidates {
            if candidate_id == entry.name {
                continue;
            }

            let key = clone_pair_key(&entry.name, &candidate_id);
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);

            if let Some(pair) = compute_clone_pair(entry, &candidate_id, &sig_map, min_similarity) {
                result.total_count += 1;
                result.type_distribution.increment(&pair.clone_type);
                result.cloned_func.insert(pair.func_a.clone());
                result.cloned_func.insert(pair.func_b.clone());

                if pair_cap == 0 || result.pairs.len() < pair_cap {
                    result.pairs.push(pair);
                }
            }
        }
    }

    // The reference sorts pairs by similarity descending with an UNSTABLE
    // sort, so equal-similarity pairs keep whatever (randomized) discovery
    // order they had — the list is order-nondeterministic there, which the
    // compat harness canonicalizes by sorting. To be deterministic on our side
    // we give equal-similarity pairs a stable tie-break on the qualified
    // names, so two identical runs are byte-identical (the harness requires
    // our determinism). The membership of the list still matches the
    // reference; only the within-tier order is pinned.
    result.pairs.sort_by(|a, b| {
        b.similarity
            .total_cmp(&a.similarity)
            .then_with(|| a.func_a.cmp(&b.func_a))
            .then_with(|| a.func_b.cmp(&b.func_b))
    });

    result
}

/// Builds an LSH index over `entries`, inserting each signature under its
/// name. Insert errors are skipped. Returns `None` if the index parameters are
/// invalid.
#[must_use]
pub fn build_index(entries: &[FuncEntry], num_bands: usize, num_rows: usize) -> Option<Index> {
    let mut idx = Index::new(num_bands, num_rows).ok()?;
    for entry in entries {
        let _ = idx.insert(entry.name.clone(), &entry.sig);
    }
    Some(idx)
}

/// Number of distinct function names across all pairs.
#[must_use]
pub fn count_distinct_funcs(pairs: &[ClonePair]) -> usize {
    let mut unique: HashSet<&str> = HashSet::with_capacity(pairs.len());
    for p in pairs {
        unique.insert(&p.func_a);
        unique.insert(&p.func_b);
    }
    unique.len()
}

/// Re-export of [`NUM_HASHES`] for callers building signatures from this module.
pub const DEFAULT_NUM_HASHES: usize = NUM_HASHES;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uast::NodeBuilder;
    use cf_uast_node::Node;

    fn method_with_receiver(token: &str) -> Node {
        let recv = NodeBuilder::new("Parameter")
            .role("Parameter")
            .token(token)
            .build();
        NodeBuilder::new("Method").child(recv).build()
    }

    #[test]
    fn count_nodes_counts_subtree() {
        let tree = NodeBuilder::new("A")
            .child(NodeBuilder::new("B").build())
            .child(
                NodeBuilder::new("C")
                    .child(NodeBuilder::new("D").build())
                    .build(),
            )
            .build();
        assert_eq!(count_nodes(&tree), 4);
    }

    #[test]
    fn is_function_node_by_type_or_roles() {
        assert!(is_function_node(&NodeBuilder::new("Function").build()));
        assert!(is_function_node(&NodeBuilder::new("Method").build()));
        let decl = NodeBuilder::new("Decl")
            .role("Function")
            .role("Declaration")
            .build();
        assert!(is_function_node(&decl));
        assert!(!is_function_node(&NodeBuilder::new("Block").build()));
    }

    #[test]
    fn extract_receiver_pointer_type() {
        let m = method_with_receiver("(f *Foo)");
        assert_eq!(extract_receiver_type(&m), "Foo");
    }

    #[test]
    fn extract_receiver_value_type() {
        let m = method_with_receiver("(f Foo)");
        assert_eq!(extract_receiver_type(&m), "Foo");
    }

    #[test]
    fn extract_receiver_missing_parts() {
        let m = method_with_receiver("(Foo)"); // single token -> no receiver
        assert_eq!(extract_receiver_type(&m), "");
    }

    #[test]
    fn extract_func_name_qualifies_methods() {
        // Entity-name precedence: "name" prop -> own token -> children[0]'s
        // token. Here the method's name comes from children[0] ("DoWork"); the
        // receiver type is discovered separately by `extract_receiver_type`
        // (which scans all children for the Parameter role), yielding
        // "Foo.DoWork".
        let name = NodeBuilder::new("Identifier")
            .role("Name")
            .token("DoWork")
            .build();
        let recv = NodeBuilder::new("Parameter")
            .role("Parameter")
            .token("(f *Foo)")
            .build();
        let m = NodeBuilder::new("Method").child(name).child(recv).build();
        assert_eq!(extract_func_name(&m), "Foo.DoWork");
    }

    #[test]
    fn extract_func_name_own_token_beats_children() {
        // The node's OWN token is checked before any child. A C `Function`
        // node whose token is the full function text keeps that text as its
        // name, so distinct functions stay distinct in the LSH index.
        let child = NodeBuilder::new("Identifier")
            .role("Name")
            .token("U16")
            .build();
        let f = NodeBuilder::new("Function")
            .token("static U16 LZ4_read16(...)")
            .child(child)
            .build();
        assert_eq!(extract_func_name(&f), "static U16 LZ4_read16(...)");
    }

    #[test]
    fn extract_func_name_falls_back_to_token_then_type() {
        let tokened = NodeBuilder::new("Function").token("plain").build();
        assert_eq!(extract_func_name(&tokened), "plain");
        let typed = NodeBuilder::new("Function").build();
        assert_eq!(extract_func_name(&typed), "Function");
    }

    #[test]
    fn count_distinct_funcs_dedups() {
        let pairs = vec![
            ClonePair {
                func_a: "a".into(),
                func_b: "b".into(),
                similarity: 1.0,
                clone_type: "Type-1".into(),
            },
            ClonePair {
                func_a: "b".into(),
                func_b: "c".into(),
                similarity: 0.9,
                clone_type: "Type-2".into(),
            },
        ];
        assert_eq!(count_distinct_funcs(&pairs), 3);
    }

    #[test]
    fn clone_pair_key_is_canonical() {
        assert_eq!(clone_pair_key("b", "a"), clone_pair_key("a", "b"));
        assert_eq!(clone_pair_key("a", "b"), ("a".into(), "b".into()));
    }
}

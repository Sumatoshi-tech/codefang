//! The MinHash + LSH clone-detection core.
//!
//! Ports the signature-building and pair-finding logic that is spread across
//! `analyzer.go` (`buildSignatures`, `extractFuncName`, `extractReceiverType`,
//! `countNodes`) and `visitor.go` (`findClonePairs`, `matchCandidates`,
//! `computeClonePair`, `clonePairResult`, `buildSignatureMap`). Keeping it in one
//! module mirrors how those functions form a single detection pipeline shared by
//! the [`crate::Analyzer`] (per-file) and the [`crate::Aggregator`] (cross-file).

use std::collections::{HashMap, HashSet};

use cf_alg_lsh::Index;
use cf_alg_minhash::Signature;
use cf_uast_node::Node;

use crate::report::{classify_clone_type, ClonePair, CloneTypeCounts};
use crate::shingler::Shingler;
use crate::uast::{
    ROLE_DECLARATION, ROLE_FUNCTION, ROLE_NAME, ROLE_PARAMETER, UAST_FUNCTION, UAST_METHOD,
};
use crate::{MIN_FUNCTION_NODES, NUM_HASHES};

/// A function's name plus its MinHash signature. Mirrors Go `funcEntry`.
#[derive(Debug, Clone)]
pub struct FuncEntry {
    /// The (possibly receiver-qualified) function name.
    pub name: String,
    /// The function's MinHash signature.
    pub sig: Signature,
}

/// Output of [`find_clone_pairs`]. Mirrors Go `clonePairResult`.
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

/// Returns `true` if `n` represents a function. Mirrors Go `isFunctionNode`.
#[must_use]
pub fn is_function_node(n: &Node) -> bool {
    n.has_any_type(&[UAST_FUNCTION, UAST_METHOD])
        || n.has_all_roles(&[ROLE_FUNCTION, ROLE_DECLARATION])
}

/// Returns the total number of nodes in a subtree. Mirrors Go `countNodes`.
#[must_use]
pub fn count_nodes(n: &Node) -> usize {
    let mut count = 1;
    for child in &n.children {
        count += count_nodes(child);
    }
    count
}

/// Extracts a unique function name from a node. Mirrors Go `extractFuncName`.
///
/// Uses the entity name (the first child with the `Name` role, falling back to
/// the node token, then the type); for methods, qualifies with the receiver type
/// (e.g. `"Foo.DoWork"`).
#[must_use]
pub fn extract_func_name(fn_node: &Node) -> String {
    let mut name = extract_entity_name(fn_node).unwrap_or_default();
    if name.is_empty() {
        name = if !fn_node.token.is_empty() {
            fn_node.token.clone()
        } else {
            fn_node.node_type.clone()
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
/// Mirrors Go `common.ExtractEntityName`: prefer the node's `props["name"]`,
/// then the first descendant carrying the `Name` role (its token), then the
/// node's own token. Returns `None` when nothing usable is found.
#[must_use]
fn extract_entity_name(n: &Node) -> Option<String> {
    if let Some(name) = n.props.get("name") {
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    // First child with the Name role whose token is non-empty.
    for child in &n.children {
        if child.has_any_role(&[ROLE_NAME]) && !child.token.is_empty() {
            return Some(child.token.clone());
        }
    }

    if !n.token.is_empty() {
        return Some(n.token.clone());
    }

    None
}

/// Extracts the receiver type name from a method node. Mirrors Go
/// `extractReceiverType`.
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

        let type_name = parts[parts.len() - 1].strip_prefix('*').unwrap_or(parts[parts.len() - 1]);
        if !type_name.is_empty() {
            return type_name.to_string();
        }
    }

    String::new()
}

/// Builds a MinHash signature for one function subtree, or `None` when the
/// function is too small or produces no shingles. Mirrors the per-function body
/// of Go `buildSignatures`.
#[must_use]
pub fn build_signature(fn_node: &Node, shingler: &Shingler, num_hashes: usize) -> Option<FuncEntry> {
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
pub fn build_signatures(functions: &[&Node], shingler: &Shingler, num_hashes: usize) -> Vec<FuncEntry> {
    let mut entries = Vec::with_capacity(functions.len());
    for fn_node in functions {
        if let Some(entry) = build_signature(fn_node, shingler, num_hashes) {
            entries.push(entry);
        }
    }
    entries
}

/// Returns a canonical key for a clone pair so `(A,B)` and `(B,A)` collide.
///
/// Mirrors Go `clonePairKey`: the two names are ordered lexicographically.
#[must_use]
fn clone_pair_key(func_a: &str, func_b: &str) -> (String, String) {
    if func_a > func_b {
        (func_b.to_string(), func_a.to_string())
    } else {
        (func_a.to_string(), func_b.to_string())
    }
}

/// Computes a clone pair between an entry and a candidate, or `None` if the
/// candidate is unknown or below `min_similarity`. Mirrors Go `computeClonePair`.
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

/// Builds a name → signature lookup. Mirrors Go `buildSignatureMap`.
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
/// `pair_cap` limits the *stored* `pairs` slice (`0` = unlimited); `total_count`
/// always reflects all unique pairs found. Mirrors Go `findClonePairs` +
/// `matchCandidates`.
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
        let Ok(candidates) = idx.query_threshold(&entry.sig, min_similarity) else {
            continue;
        };

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

    // Go: sort.Slice(pairs, |i,j| pairs[i].Similarity > pairs[j].Similarity).
    // A strict-greater comparator over f64 (no NaN here) gives a descending
    // sort; `sort_by` with `total_cmp` reversed reproduces it deterministically.
    result
        .pairs
        .sort_by(|a, b| b.similarity.total_cmp(&a.similarity));

    result
}

/// Builds an LSH index over `entries`, inserting each signature under its name.
///
/// Insert errors are skipped (mirroring Go's `continue` on `idx.Insert` error).
/// Returns `None` if the index parameters are invalid.
#[must_use]
pub fn build_index(entries: &[FuncEntry], num_bands: usize, num_rows: usize) -> Option<Index> {
    let mut idx = Index::new(num_bands, num_rows).ok()?;
    for entry in entries {
        let _ = idx.insert(entry.name.clone(), &entry.sig);
    }
    Some(idx)
}

/// Number of distinct function names across all pairs. Mirrors Go
/// `countDistinctFuncs`.
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
        let recv = NodeBuilder::new("Parameter").role("Parameter").token(token).build();
        NodeBuilder::new("Method").child(recv).build()
    }

    #[test]
    fn count_nodes_counts_subtree() {
        let tree = NodeBuilder::new("A")
            .child(Node::new("B"))
            .child(NodeBuilder::new("C").child(Node::new("D")).build())
            .build();
        assert_eq!(count_nodes(&tree), 4);
    }

    #[test]
    fn is_function_node_by_type_or_roles() {
        assert!(is_function_node(&Node::new("Function")));
        assert!(is_function_node(&Node::new("Method")));
        let decl = NodeBuilder::new("Decl").role("Function").role("Declaration").build();
        assert!(is_function_node(&decl));
        assert!(!is_function_node(&Node::new("Block")));
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
        let recv = NodeBuilder::new("Parameter").role("Parameter").token("(f *Foo)").build();
        let name = NodeBuilder::new("Identifier").role("Name").token("DoWork").build();
        let m = NodeBuilder::new("Method").child(recv).child(name).build();
        assert_eq!(extract_func_name(&m), "Foo.DoWork");
    }

    #[test]
    fn extract_func_name_falls_back_to_token_then_type() {
        let tokened = NodeBuilder::new("Function").token("plain").build();
        assert_eq!(extract_func_name(&tokened), "plain");
        let typed = Node::new("Function");
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

//! The clone-detection core: per-function entries, candidate generation
//! through the LSH index, exact verification, classification, and the
//! bounded top-k pair list.
//!
//! Pairs are enumerated exactly once by querying every function in id order
//! for the candidates *after* it ([`cf_alg_lsh::Index::query_after`]), so no
//! `seen` set is needed and the work is linear in the number of candidate
//! pairs. The stored pair list is a bounded heap keyed on
//! `(similarity desc, func_a asc, func_b asc)`, so the capped detail is
//! always the best pairs under that total order, independent of discovery
//! order.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use cf_alg_lsh::{Index, Scratch};
use cf_alg_minhash::Signature;
use cf_uast_node::Node;

use crate::profile::{profile, Fingerprint};
use crate::report::{ClonePair, CloneType, CloneTypeCounts};
use crate::shingler::Shingler;
use crate::MIN_FUNCTION_NODES;

/// One clone candidate as produced by the [`crate::Visitor`] for a single
/// file.
#[derive(Debug, Clone)]
pub struct FuncEntry {
    /// The (receiver-qualified or scope-qualified) function name.
    pub name: String,
    /// `MinHash` signature of the shingle set (the LSH input).
    pub sig: Signature,
    /// Sorted, deduplicated FNV-1a hashes of the function's shingles.
    pub shingles: Vec<u64>,
    /// Fingerprint of the node-type sequence.
    pub structure: Fingerprint,
    /// Fingerprint of the leaf-token sequence.
    pub tokens: Fingerprint,
    /// Index (within the same file scan) of the nearest enclosing candidate,
    /// for closures nested inside another candidate.
    pub parent: Option<usize>,
}

impl FuncEntry {
    /// Re-bases the entry into the global candidate list: `file` is the
    /// file's index, `base` the global index of the file's first entry.
    #[must_use]
    pub fn into_indexed(self, file: usize, base: usize) -> IndexedFunc {
        IndexedFunc {
            name: self.name,
            file,
            sig: self.sig,
            shingles: self.shingles,
            structure: self.structure,
            tokens: self.tokens,
            parent: self.parent.map(|p| p + base),
        }
    }
}

/// A clone candidate in the global (cross-file) candidate list.
#[derive(Debug, Clone)]
pub struct IndexedFunc {
    /// `file::name` — the name qualified with its source file.
    pub name: String,
    /// Index of the source file in the aggregator's file list.
    pub file: usize,
    /// `MinHash` signature of the shingle set.
    pub sig: Signature,
    /// Sorted, deduplicated shingle hashes.
    pub shingles: Vec<u64>,
    /// Fingerprint of the node-type sequence.
    pub structure: Fingerprint,
    /// Fingerprint of the leaf-token sequence.
    pub tokens: Fingerprint,
    /// Global index of the nearest enclosing candidate, if nested.
    pub parent: Option<usize>,
}

/// Builds the entry for one function subtree, or `None` when the function
/// has fewer than [`MIN_FUNCTION_NODES`] nodes or yields no shingles.
#[must_use]
pub fn build_entry(
    name: String,
    fn_node: &Node,
    shingler: &Shingler,
    num_hashes: usize,
) -> Option<FuncEntry> {
    let profile = profile(fn_node);
    if profile.node_count < MIN_FUNCTION_NODES {
        return None;
    }
    let shingle_set = shingler.shingle_hashes(&profile.types);
    if shingle_set.is_empty() {
        return None;
    }
    let mut sig = Signature::new(num_hashes).ok()?;
    for &h in &shingle_set {
        sig.add_hash(h);
    }
    Some(FuncEntry {
        name,
        sig,
        shingles: shingle_set,
        structure: profile.structure,
        tokens: profile.tokens,
        parent: None,
    })
}

/// Exact Jaccard index of two sorted, deduplicated hash sets.
///
/// ```
/// use cf_clones::engine::jaccard;
/// assert!((jaccard(&[1, 2, 3, 4], &[3, 4, 5, 6]) - 2.0 / 6.0).abs() < 1e-12);
/// assert_eq!(jaccard(&[1, 2], &[1, 2]), 1.0);
/// assert_eq!(jaccard(&[], &[]), 0.0);
/// ```
#[must_use]
pub fn jaccard(a: &[u64], b: &[u64]) -> f64 {
    let (mut i, mut j, mut shared) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    let union = a.len() + b.len() - shared;
    if union == 0 {
        return 0.0;
    }
    shared as f64 / union as f64
}

/// Classifies a candidate pair: identical structure is Type-1 (same tokens)
/// or Type-2 (different tokens) at similarity `1.0`; otherwise the exact
/// shingle Jaccard must reach `threshold` for a Type-3 pair.
#[must_use]
pub fn classify_pair(a: &IndexedFunc, b: &IndexedFunc, threshold: f64) -> Option<(CloneType, f64)> {
    if a.structure == b.structure {
        let clone_type = if a.tokens == b.tokens {
            CloneType::Type1
        } else {
            CloneType::Type2
        };
        return Some((clone_type, 1.0));
    }
    let similarity = jaccard(&a.shingles, &b.shingles);
    (similarity >= threshold).then_some((CloneType::Type3, similarity))
}

/// `true` when `ancestor` encloses `descendant` (a closure and the function
/// it is written in are never a clone pair of each other).
fn is_ancestor(funcs: &[IndexedFunc], ancestor: usize, descendant: usize) -> bool {
    let mut cursor = funcs[descendant].parent;
    while let Some(p) = cursor {
        if p == ancestor {
            return true;
        }
        cursor = funcs[p].parent;
    }
    false
}

/// A pair kept in the bounded top-k heap. Its [`Ord`] ranks a WORSE pair as
/// greater — lower similarity first, then the later name — so the max-heap's
/// top is the pair to evict and an ascending sort lists the best pair first.
struct Ranked<'a> {
    similarity: f64,
    clone_type: CloneType,
    a: usize,
    b: usize,
    name_a: &'a str,
    name_b: &'a str,
}

impl Ord for Ranked<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .similarity
            .total_cmp(&self.similarity)
            .then_with(|| self.name_a.cmp(other.name_a))
            .then_with(|| self.name_b.cmp(other.name_b))
    }
}

impl PartialOrd for Ranked<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Ranked<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Ranked<'_> {}

/// Output of [`find_clone_pairs`].
#[derive(Debug, Default)]
pub struct ClonePairResult {
    /// The stored pairs, best first (similarity desc, then name).
    pub pairs: Vec<ClonePair>,
    /// `(file of func_a, file of func_b)` for every stored pair.
    pub pair_files: Vec<(usize, usize)>,
    /// Exact number of pairs found, regardless of the cap.
    pub total_count: usize,
    /// Exact per-type distribution over every pair found.
    pub distribution: CloneTypeCounts,
    /// Per candidate: whether it takes part in at least one pair.
    pub cloned: Vec<bool>,
}

/// Queries the index for every candidate, verifies and classifies each
/// candidate pair, and keeps the best `pair_cap` pairs (`0` = unlimited).
#[must_use]
pub fn find_clone_pairs(
    funcs: &[IndexedFunc],
    index: &Index,
    pair_cap: usize,
    threshold: f64,
) -> ClonePairResult {
    let mut scratch = Scratch::default();
    let mut heap: BinaryHeap<Ranked<'_>> = BinaryHeap::new();
    let mut result = ClonePairResult {
        cloned: vec![false; funcs.len()],
        ..ClonePairResult::default()
    };

    for (i, f) in funcs.iter().enumerate() {
        let Ok(candidates) = index.query_after(&f.sig, i, &mut scratch) else {
            continue;
        };
        for j in candidates {
            let g = &funcs[j];
            if is_ancestor(funcs, i, j) {
                continue;
            }
            let Some((clone_type, similarity)) = classify_pair(f, g, threshold) else {
                continue;
            };
            result.total_count += 1;
            result.distribution.increment(clone_type);
            result.cloned[i] = true;
            result.cloned[j] = true;

            let (a, b) = if f.name <= g.name { (i, j) } else { (j, i) };
            heap.push(Ranked {
                similarity,
                clone_type,
                a,
                b,
                name_a: &funcs[a].name,
                name_b: &funcs[b].name,
            });
            if pair_cap > 0 && heap.len() > pair_cap {
                heap.pop();
            }
        }
    }

    for r in heap.into_sorted_vec() {
        result.pairs.push(ClonePair {
            func_a: r.name_a.to_string(),
            func_b: r.name_b.to_string(),
            similarity: r.similarity,
            clone_type: r.clone_type,
        });
        result.pair_files.push((funcs[r.a].file, funcs[r.b].file));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NUM_BANDS, NUM_HASHES, NUM_ROWS};

    /// A candidate over the given shingle hashes with its own token
    /// fingerprint derived from `token_salt`.
    fn func(name: &str, file: usize, shingles: &[u64], token_salt: u8) -> IndexedFunc {
        let mut sig = Signature::new(NUM_HASHES).unwrap();
        for &h in shingles {
            sig.add_hash(h);
        }
        let structure = fingerprint_of(shingles);
        let mut salted: Vec<u64> = shingles.to_vec();
        salted.push(u64::from(token_salt));
        IndexedFunc {
            name: name.to_string(),
            file,
            sig,
            shingles: shingles.to_vec(),
            structure,
            tokens: fingerprint_of(&salted),
            parent: None,
        }
    }

    fn fingerprint_of(values: &[u64]) -> Fingerprint {
        let mut n = cf_uast_node::Builder::new().with_type("Function").build();
        for v in values {
            n.add_child(
                cf_uast_node::Builder::new()
                    .with_type("Identifier")
                    .with_token(v.to_string())
                    .build(),
            );
        }
        profile(&n).tokens
    }

    fn index_of(funcs: &[IndexedFunc]) -> Index {
        let mut idx = Index::new(NUM_BANDS, NUM_ROWS).unwrap();
        for (i, f) in funcs.iter().enumerate() {
            idx.insert(i, &f.sig).unwrap();
        }
        idx
    }

    fn range(lo: u64, hi: u64) -> Vec<u64> {
        (lo..hi).collect()
    }

    #[test]
    fn jaccard_of_sorted_sets() {
        assert!((jaccard(&[1, 2, 3, 4], &[3, 4, 5, 6]) - 1.0 / 3.0).abs() < 1e-12);
        assert!((jaccard(&[1], &[2]) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn identical_structure_and_tokens_is_type1() {
        let a = func("a", 0, &range(0, 50), 0);
        let b = func("b", 1, &range(0, 50), 0);
        assert_eq!(classify_pair(&a, &b, 0.5), Some((CloneType::Type1, 1.0)));
    }

    #[test]
    fn identical_structure_different_tokens_is_type2() {
        let a = func("a", 0, &range(0, 50), 0);
        let b = func("b", 1, &range(0, 50), 1);
        assert_eq!(classify_pair(&a, &b, 0.5), Some((CloneType::Type2, 1.0)));
    }

    #[test]
    fn different_structure_is_type3_only_above_threshold() {
        let a = func("a", 0, &range(0, 60), 0);
        let near = func("b", 1, &range(20, 80), 0); // 40 shared / 80 union = 0.5
        let far = func("c", 2, &range(50, 110), 0); // 10 / 110
        let (ct, sim) = classify_pair(&a, &near, 0.5).expect("found");
        assert_eq!(ct, CloneType::Type3);
        assert!((sim - 0.5).abs() < 1e-12);
        assert_eq!(classify_pair(&a, &far, 0.5), None);
    }

    #[test]
    fn pairs_are_found_once_best_first_and_names_sorted() {
        let funcs = vec![
            func("z", 0, &range(0, 50), 0),
            func("m", 1, &range(0, 50), 0),
            func("a", 2, &range(10, 60), 0), // 0.667 with the others
        ];
        let idx = index_of(&funcs);
        let r = find_clone_pairs(&funcs, &idx, 0, 0.5);
        assert_eq!(r.total_count, 3);
        assert_eq!(r.distribution.type1, 1);
        assert_eq!(r.distribution.type3, 2);
        assert!(r.cloned.iter().all(|c| *c));
        let names: Vec<String> = r.pairs.iter().map(ClonePair::display_name).collect();
        assert_eq!(names, vec!["m <-> z", "a <-> m", "a <-> z"]);
        assert_eq!(r.pair_files, vec![(1, 0), (2, 1), (2, 0)]);
    }

    #[test]
    fn pair_cap_keeps_the_best_pairs_and_the_exact_total() {
        let funcs: Vec<IndexedFunc> = (0..5)
            .map(|i| func(&format!("f{i}"), i, &range(0, 50), 0))
            .collect();
        let idx = index_of(&funcs);
        let r = find_clone_pairs(&funcs, &idx, 3, 0.5);
        assert_eq!(r.total_count, 10);
        assert_eq!(r.distribution.type1, 10);
        let names: Vec<String> = r.pairs.iter().map(ClonePair::display_name).collect();
        assert_eq!(names, vec!["f0 <-> f1", "f0 <-> f2", "f0 <-> f3"]);
    }

    #[test]
    fn a_closure_is_never_paired_with_its_enclosing_function() {
        let outer = func("outer", 0, &range(0, 60), 0);
        let mut inner = func("outer.func#1", 0, &range(5, 60), 0);
        inner.parent = Some(0);
        let funcs = vec![outer, inner];
        let idx = index_of(&funcs);
        let r = find_clone_pairs(&funcs, &idx, 0, 0.5);
        assert_eq!(r.total_count, 0, "{:?}", r.pairs);
    }

    #[test]
    fn no_self_pairs() {
        let funcs = vec![func("only", 0, &range(0, 50), 0)];
        let idx = index_of(&funcs);
        let r = find_clone_pairs(&funcs, &idx, 0, 0.5);
        assert_eq!(r.total_count, 0);
        assert_eq!(r.cloned, vec![false]);
    }
}

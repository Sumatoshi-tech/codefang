# Understanding clone detection

This page explains the mental model behind the clone detection analyzer: what
a clone pair is, the clone-type taxonomy, and why the two-stage MinHash + LSH
design keeps cross-file detection fast. For the output schema and the metric
definitions, see the [Clones reference](../analyzers/clones.md).

---

## What it measures

The clone detection analyzer finds **duplicate and near-duplicate functions**
across your entire codebase. It operates on the UAST representation and
detects clones **cross-file** — not just within a single file.

### Clone pairs

A clone pair is two functions whose AST structure is similar enough to be
considered duplicated code. Each pair carries a **similarity** (the exact
Jaccard index of the two functions' shingle sets, 0.5–1.0) and a **clone
type**.

### Clone types

The classification follows the standard taxonomy (Roy & Cordy) and is decided
from two fingerprints per function — one over its *structure* (the sequence of
AST node types) and one over its *tokens* (the identifiers, literals and
operators at the leaves) — plus the exact shingle similarity:

| Type | Rule | Similarity |
|---|---|---|
| **Type-1 (Exact)** | identical structure **and** identical tokens | 1.0 |
| **Type-2 (Renamed)** | identical structure, different identifiers / literals | 1.0 |
| **Type-3 (Near-miss)** | different structure, shingle Jaccard ≥ 0.5 | 0.5 – 1.0 |

So a function copied verbatim is Type-1; the same copy with every variable
renamed, a constant changed or an operator swapped is Type-2; a copy with a
statement added or removed is Type-3 with a similarity that says how much of
the structure survived. Comments are ignored entirely: a copy that only
gained comments is still Type-1.

### Clone ratio

The share of functions that take part in at least one clone pair, computed
globally across all files:

```text
clone_ratio = functions in ≥ 1 pair / total_functions
```

It is bounded to `[0, 1]` (a pair-count ratio is not: three copies of one
function form three pairs). Both the status message and the `N/10` score are
keyed off it:

!!! info "Interpretation"
    - **0.0** — `No code clones detected`, 10/10
    - **≤ 0.1** — `Low duplication`
    - **≤ 0.3** — `Moderate duplication - consider refactoring clone pairs`
    - **> 0.3** — `High duplication - significant refactoring recommended`

    The score is `1 − clone_ratio`, so the message and the score always agree.

---

## Methodology

### Stage 1: per-file profiles and signatures

For each source file, the analyzer:

1. **Parses** the file into a UAST and finds every function, method and
   closure (UAST `Lambda`). Closures are named after their enclosing function
   and ordinal (`Handle.func#2`), or `func#N` at file level.
2. **Profiles** each function: a pre-order traversal of its subtree (comment
   nodes skipped) yields the node-type sequence, a *structure* fingerprint
   over that sequence, and a *token* fingerprint over the leaf
   `(type, token)` pairs. Functions with fewer than 20 nodes are counted but
   not profiled further.
3. **Shingles** the type sequence into sliding windows of `k = 5` consecutive
   node types. For example, `[Function, Block, Assignment, Identifier, Call,
   Return]` produces `Function|Block|Assignment|Identifier|Call` and
   `Block|Assignment|Identifier|Call|Return`. Each shingle is hashed; the
   sorted, deduplicated hash set is kept — it is the exact set the next stage
   verifies against.
4. **MinHash** compresses that shingle set into a fixed-size signature of 128
   hash values.

No comparison happens at this stage; it is deferred to the aggregation stage.

### Stage 2: cross-file clone detection

After all files have been processed, the aggregator:

1. **Qualifies** function names with their source file path
   (`path/to/file.go::FuncName`) so that same-named functions across files
   are distinguishable.
2. **Builds a global LSH index** (32 bands × 4 rows) from all signatures.
   LSH hashes each signature's bands into buckets — functions whose
   signatures share any bucket are candidate clones. With these parameters a
   pair with Jaccard `s` becomes a candidate with probability
   `1 − (1 − s⁴)³²`: ≈ 0.87 at the 0.5 threshold, > 0.99 at 0.64, and
   < 0.06 at 0.2. (The previous 16 × 8 layout had a recall of 0.06 at 0.5,
   which is why near-miss pairs went unreported.)
3. **Verifies every candidate pair exactly**: equal structure fingerprints
   mean similarity 1.0 — Type-1 if the token fingerprints match, Type-2
   otherwise; different structure means the exact Jaccard of the two shingle
   sets, reported as Type-3 when it reaches 0.5 and dropped otherwise. The
   `similarity` in the report is therefore always exact, never the MinHash
   estimate, and the lower S-curve never adds false positives.
4. **Counts** every pair, keeps the best 1000 (similarity descending, then
   name) as the detail list, and computes `clone_ratio` from the functions
   that appear in at least one pair.

Every unordered pair is enumerated exactly once (each function is queried for
the candidates that come after it in the index), so the work is linear in the
number of candidate pairs and nothing is allocated per pair: 4000 identical
functions — 8 million Type-1 pairs — are counted in a few seconds.

This two-stage design means clone detection works across file boundaries. A
controller duplicated in `pkg/a/handler.go` and `pkg/b/handler.go` is detected
even though each file is parsed independently.

### Why MinHash + LSH?

Pairwise comparison of N functions is O(N²). For a project with 100K
functions this is infeasible. MinHash compresses each function into a 1 KB
signature (128 hashes × 8 bytes), and LSH reduces candidate pair generation to
near-linear time by only comparing functions that hash into the same bucket.
The exact verification step then makes the result precise: LSH only decides
*which* pairs are looked at, never what is reported about them.

---

## Use cases

- **Refactoring targets**: Identify exact and near-duplicate functions that should be consolidated into shared utilities.
- **Code review gates**: Flag pull requests that introduce new clone pairs, especially Type-1 (exact) duplicates.
- **Copy-paste detection**: Find controllers, handlers, or boilerplate that were copy-pasted across packages instead of being extracted.
- **Technical debt tracking**: Monitor clone ratio over time to ensure duplication does not grow unbounded.
- **Cross-team coordination**: Detect when multiple teams independently implement the same logic in different packages.

---

## Methodology references

- **MinHash**: Broder, A. Z. (1997). *On the resemblance and containment of documents*. Proceedings of the Compression and Complexity of Sequences. MinHash estimates Jaccard similarity between sets using compact fixed-size signatures.
- **LSH (Locality-Sensitive Hashing)**: Indyk, P. & Motwani, R. (1998). *Approximate nearest neighbors: towards removing the curse of dimensionality*. STOC. Banded LSH reduces candidate pair generation from O(N²) to near-linear time.
- **AST-based clone detection**: Jiang, L. et al. (2007). *DECKARD: Scalable and accurate tree-based detection of code clones*. ICSE. Tree-based shingling captures structural similarity independent of identifier names.
- **Clone taxonomy**: Roy, C. K. & Cordy, J. R. (2007). *A survey on software clone detection research*. The Type-1/Type-2/Type-3 classification follows the standard clone taxonomy.

---

## Limitations

- **Language scope**: Works with any language supported by the UAST parser. Unsupported files are silently skipped.
- **Structural similarity**: Type-3 similarity is based on AST node types, not token values. Two functions with the same nesting shape but different semantics (e.g. add vs multiply) are reported as Type-2 clones, which is what the taxonomy defines.
- **Minimum function size**: Functions with fewer than 20 UAST nodes (comments excluded) are counted in `total_functions` but never paired.
- **Probabilistic candidate recall**: LSH is a probabilistic candidate stage. A pair whose similarity is exactly at the 0.5 threshold is proposed with probability ≈ 0.87; pairs at 0.64 and above are proposed practically always, and Type-1/Type-2 pairs always. Every proposed pair is verified exactly, so the stage never produces false positives.
- **Generated code**: The analyzer does not distinguish hand-written code from generated code; generated files are excluded by the default path policy (`--include-generated` adds them).
- **No Type-4 clones**: Semantic clones (different structure, same behavior) are not detected. The analyzer operates purely on syntactic structure.

---

## See also

- [Clones reference](../analyzers/clones.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

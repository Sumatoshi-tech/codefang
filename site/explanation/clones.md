# Understanding clone detection

This page explains the mental model behind the clone detection analyzer: what
a clone pair is, the clone-type taxonomy, and why the two-stage MinHash + LSH
design keeps cross-file detection fast. For configuration keys and the output
schema, see the [Clones reference](../analyzers/clones.md).

---

## What it measures

The clone detection analyzer finds **duplicate and near-duplicate functions**
across your entire codebase using MinHash signatures and Locality-Sensitive
Hashing (LSH). It operates on the UAST representation and detects clones
**cross-file** — not just within a single file.

### Clone pairs

A clone pair is two functions whose AST structure is similar enough to be considered duplicated code. Each pair includes a **similarity score** (0.0–1.0) and a **clone type** classification.

### Clone types

| Type | Similarity | Description |
|---|---|---|
| **Type-1 (Exact)** | = 1.0 | Identical AST structure and tokens |
| **Type-2 (Renamed)** | &ge; 0.8 | Identical structure, different variable/function names |
| **Type-3 (Near-miss)** | &ge; 0.5 | Similar but modified structure |

### Clone ratio

The ratio of detected clone pairs to total functions, computed globally across all files:

```
clone_ratio = total_clone_pairs / total_functions
```

!!! info "Interpretation"
    - **0.0** (green): No duplication detected
    - **&le; 0.1** (yellow): Low duplication — few clone pairs
    - **&le; 0.3** (red): Moderate duplication — consider refactoring
    - **> 0.3**: High duplication — significant refactoring recommended

---

## Methodology

### Stage 1: per-file signature extraction

For each source file, the analyzer:

1. **Parses** the file into a UAST and finds all function/method nodes.
2. **Shingles** each function by performing a pre-order traversal of the AST subtree and extracting sliding windows of `k` consecutive node types (k=5 by default). For example, a function with nodes `[Function, Block, Assignment, Identifier, Call, Return]` produces shingles like `Function|Block|Assignment|Identifier|Call` and `Block|Assignment|Identifier|Call|Return`.
3. **MinHash** compresses each function's shingle set into a fixed-size signature of 128 hash values. Two functions with identical structure produce identical signatures; similar functions produce similar signatures.

Signatures are exported per-file along with function names. At this stage, no clone detection occurs — comparison is deferred to the aggregation stage.

### Stage 2: cross-file clone detection

After all files have been processed, the aggregator:

1. **Qualifies** function names with their source file path (`path/to/file.go::FuncName`) so that same-named functions across files are distinguishable.
2. **Builds a global LSH index** (16 bands, 8 rows per band) from all collected signatures. LSH hashes each signature's bands into buckets — functions whose signatures share any bucket are candidate clones.
3. **Queries** the LSH index for each function, retrieves candidates above the similarity threshold (0.5), computes exact Jaccard similarity from the full MinHash signatures, and classifies each pair by clone type.
4. **Computes metrics** globally: `clone_ratio = total_pairs / total_functions`.

This two-stage design means clone detection works across file boundaries. A controller duplicated in `pkg/a/handler.go` and `pkg/b/handler.go` is detected even though each file is parsed independently.

### Why MinHash + LSH?

Pairwise comparison of N functions is O(N&sup2;). For a project with 100K functions this is infeasible. MinHash compresses each function into a 1 KB signature (128 hashes &times; 8 bytes), and LSH reduces candidate pair generation to near-linear time by only comparing functions that hash into the same bucket. The combination provides probabilistic guarantees on recall while keeping the analysis fast.

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
- **LSH (Locality-Sensitive Hashing)**: Indyk, P. & Motwani, R. (1998). *Approximate nearest neighbors: towards removing the curse of dimensionality*. STOC. Banded LSH reduces candidate pair generation from O(N&sup2;) to near-linear time.
- **AST-based clone detection**: Jiang, L. et al. (2007). *DECKARD: Scalable and accurate tree-based detection of code clones*. ICSE. Tree-based shingling captures structural similarity independent of identifier names.
- **Clone taxonomy**: Roy, C. K. & Cordy, J. R. (2007). *A survey on software clone detection research*. The Type-1/Type-2/Type-3 classification follows the standard clone taxonomy.

---

## Limitations

- **Language scope**: Works with any language supported by the UAST parser. Unsupported files are silently skipped.
- **Structural only**: Detection is based on AST node types, not token values. Two functions with identical structure but completely different semantics (e.g., add vs multiply with the same nesting shape) may appear as clones.
- **Minimum function size**: Functions smaller than the shingle window (5 AST nodes) produce no shingles and are excluded from clone detection. They are still counted in `total_functions`.
- **Probabilistic recall**: LSH is a probabilistic algorithm. Clone pairs with similarity just above the threshold (0.5) may occasionally be missed. High-similarity pairs (Type-1 and Type-2) have near-perfect recall.
- **Generated code**: The analyzer does not distinguish hand-written code from generated code. Consider excluding generated directories.
- **No Type-4 clones**: Semantic clones (different structure, same behavior) are not detected. The analyzer operates purely on syntactic structure.

---

## See also

- [Clones reference](../analyzers/clones.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

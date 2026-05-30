# Understanding structural hotness

This page explains the mental model behind the shotness analyzer: what
node-level change frequency and coupling mean, how coupling strength is
computed, and how the aggregation pipeline is structured. For configuration
keys and the output schema, see the
[Shotness reference](../analyzers/shotness.md).

---

## What it measures

The shotness analyzer measures **structural hotness** -- the change frequency of
individual code entities (functions, methods, classes) across Git history.
Unlike the couples analyzer which operates at file granularity, shotness
operates at the UAST node level, providing fine-grained co-change analysis.

### Node change frequency

For each code entity matched by the DSL query (functions by default), the analyzer counts how many commits modified lines within that entity's span. Entities that change frequently are "hot" -- they are likely volatile, complex, or central to the system.

### Node co-change coupling

When two code entities are modified in the same commit, their coupling counter is incremented. This produces a fine-grained coupling matrix at the function level, which is more precise than file-level coupling from the couples analyzer.

### Coupling strength

Coupling strength is normalized to a 0-1 scale using the formula:

```text
strength(A, B) = co_changes(A, B) / max(co_changes(A, B), changes(A), changes(B))
```

This ensures the result is always in [0, 1] and provides a meaningful confidence metric. A strength of 1.0 means functions always change together; 0.5 means they co-change half the time relative to the most active function.

### Risk classification

Nodes are classified into risk levels based on absolute change counts:

| Risk Level | Threshold | Meaning |
|---|---|---|
| **HIGH** | ≥ 20 changes | Requires immediate attention and robust test coverage |
| **MEDIUM** | ≥ 10 changes | Should be monitored and potentially refactored |
| **LOW** | < 10 changes | Normal change frequency |

### How it works

For each commit:

1. Parse the before and after versions of each changed file into UAST
2. Apply the `dsl_struct` query to select target nodes (e.g., functions)
3. Apply the `dsl_name` query to extract the name of each node
4. Map diff hunks to nodes using line-range overlaps
5. Emit a per-commit TC (Transient Commit result) with touched node deltas and coupling pairs

After all commits are processed, the Aggregator accumulates TCs into a final report with sorted nodes and a sparse co-change matrix.

### Architecture

The shotness analyzer follows the **TC/Aggregator** pattern:

- **Consume phase**: Per-commit processing builds working state (`nodes`, `files` maps for deletion/rename tracking) and emits a `TC{Data: *CommitData}` with node touch deltas and coupling pairs.
- **Aggregation phase**: The `Aggregator` accumulates node counts and coupling matrices from the TC stream. It supports disk-backed spilling via `SpillStore` for memory-bounded operation.
- **Serialization phase**: `SerializeTICKs()` converts aggregated tick data into the `Nodes`/`Counters` report consumed by `ComputeAllMetrics()` and plot generation.

The `nodes` map remains in the analyzer as working state because `handleDeletion`, `handleInsertion`, `handleModification`, and `applyRename` read and mutate it during `Consume()`. The aggregator maintains its own separate accumulation of counts and couplings.

---

## Use cases

- **Function-level hotspot detection**: Find the most frequently changed functions in the codebase. These are the highest-risk points for bugs.
- **Fine-grained coupling analysis**: Discover which functions always change together. This reveals implicit dependencies that file-level coupling misses.
- **Refactoring prioritization**: Functions that are both hot (high change count) and coupled (always change with others) are the best refactoring candidates.
- **Architecture validation**: Functions from different packages that are highly coupled may indicate a leaking abstraction.
- **Test prioritization**: Focus testing resources on the hottest functions.

---

## Interpreting results

### Reading the coupling strength

| Strength | Interpretation |
|---|---|
| 0.8 - 1.0 | Very tight coupling. Functions almost always change together. Consider merging or extracting shared logic. |
| 0.5 - 0.8 | Moderate coupling. There is a significant shared dependency. Review if coupling is intentional. |
| 0.2 - 0.5 | Loose coupling. Occasional co-changes, likely due to shared APIs or data structures. |
| < 0.2 | Minimal coupling. Co-changes are incidental. |

### Actionable insights

1. **High hotness + High coupling**: Core function that drives many changes. Candidate for splitting or stabilizing the interface.
2. **High hotness + Low coupling**: Frequently bugfixed isolated function. Needs better tests and potentially a redesign.
3. **Low hotness + High coupling**: Stable function that always changes with others. Check if coupling is necessary or indicates a design smell.

---

## Limitations

- **UAST required**: Only languages with UAST parser support are analyzed. Files in unsupported languages are skipped entirely.
- **CPU intensive**: The analyzer performs UAST parsing on both the before and after versions of every changed file in every commit. This makes it one of the most expensive analyzers. It benefits from parallel execution.
- **Name collisions**: If two functions in different files have the same name, they are tracked as distinct nodes (the file path is part of the key). However, if a file is renamed, the analyzer updates all associated nodes.
- **Shallow extraction within a file**: When multiple structural nodes in the same file share the same extracted name (e.g., nested functions with identical names), only one is tracked. The last one encountered wins. Qualified paths (e.g., `OuterClass.innerMethod`) are not built.
- **DSL limitations**: The DSL query must match nodes that have position information (`Pos` field) in the UAST. Nodes without position data cannot be mapped to diff hunks.
- **Large functions**: A change anywhere within a function's line range counts as a change to that function. Very large functions (hundreds of lines) will have inflated change counts.

---

## See also

- [Shotness reference](../analyzers/shotness.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

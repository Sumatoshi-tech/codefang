# Shotness Analyzer

The shotness analyzer measures **structural hotness** -- the change frequency of individual code entities (functions, methods, classes) across Git history. Unlike the couples analyzer which operates at file granularity, shotness operates at the UAST node level, providing fine-grained co-change analysis.

---

## Quick Start

```bash
codefang run -a history/shotness .
```

With custom node selection:

```bash
codefang run -a history/shotness \
  --shotness-dsl-struct 'filter(.roles has "Function")' \
  --shotness-dsl-name '.props.name' \
  .
```

!!! note "Requires UAST"
    The shotness analyzer needs UAST support to identify code structures. It is automatically enabled when the UAST pipeline is available.

---

## What It Measures

### Node Change Frequency

For each code entity matched by the DSL query (functions by default), the analyzer counts how many commits modified lines within that entity's span. Entities that change frequently are "hot" -- they are likely volatile, complex, or central to the system.

### Node Co-Change Coupling

When two code entities are modified in the same commit, their coupling counter is incremented. This produces a fine-grained coupling matrix at the function level, which is more precise than file-level coupling from the couples analyzer.

### How It Works

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

## Configuration Options

| Option | Type | Default | Description |
|---|---|---|---|
| `Shotness.DSLStruct` | `string` | `filter(.roles has "Function")` | UAST DSL query to select which code structures to track. |
| `Shotness.DSLName` | `string` | `.props.name` | UAST DSL expression to extract the name from each matched node. |

```yaml
# .codefang.yml
history:
  shotness:
    dsl_struct: 'filter(.roles has "Function")'
    dsl_name: '.props.name'
```

### Custom DSL Examples

=== "Track classes instead of functions"

    ```yaml
    dsl_struct: 'filter(.roles has "Class")'
    dsl_name: '.props.name'
    ```

=== "Track both functions and methods"

    ```yaml
    dsl_struct: 'filter(.roles has "Function" or .roles has "Method")'
    dsl_name: '.props.name'
    ```

=== "Track interfaces"

    ```yaml
    dsl_struct: 'filter(.roles has "Interface")'
    dsl_name: '.props.name'
    ```

---

## Example Output

=== "JSON"

    ```json
    {
      "nodes": [
        {
          "type": "Function",
          "name": "processFile",
          "file": "pkg/core/engine.go"
        },
        {
          "type": "Function",
          "name": "validate",
          "file": "pkg/core/engine.go"
        },
        {
          "type": "Function",
          "name": "handleRequest",
          "file": "pkg/api/handler.go"
        }
      ],
      "counters": [
        {"0": 42, "1": 15, "2": 8},
        {"0": 15, "1": 28, "2": 3},
        {"0": 8,  "1": 3,  "2": 35}
      ]
    }
    ```

    The `counters` array is a sparse co-change matrix. `counters[i][i]` is the total change count for node `i`. `counters[i][j]` (where `i != j`) is the co-change count between nodes `i` and `j`.

=== "YAML"

    ```yaml
    nodes:
      - type: Function
        name: processFile
        file: pkg/core/engine.go
      - type: Function
        name: validate
        file: pkg/core/engine.go
    counters:
      - 0: 42
        1: 15
      - 0: 15
        1: 28
    ```

---

## Use Cases

- **Function-level hotspot detection**: Find the most frequently changed functions in the codebase. These are the highest-risk points for bugs.
- **Fine-grained coupling analysis**: Discover which functions always change together. This reveals implicit dependencies that file-level coupling misses.
- **Refactoring prioritization**: Functions that are both hot (high change count) and coupled (always change with others) are the best refactoring candidates.
- **Architecture validation**: Functions from different packages that are highly coupled may indicate a leaking abstraction.
- **Test prioritization**: Focus testing resources on the hottest functions.

---

## Limitations

- **UAST required**: Only languages with UAST parser support are analyzed. Files in unsupported languages are skipped entirely.
- **CPU intensive**: The analyzer performs UAST parsing on both the before and after versions of every changed file in every commit. This makes it one of the most expensive analyzers. It benefits from parallel execution.
- **Name collisions**: If two functions in different files have the same name, they are tracked as distinct nodes (the file path is part of the key). However, if a file is renamed, the analyzer updates all associated nodes.
- **Shallow extraction within a file**: When multiple structural nodes in the same file share the same extracted name (e.g., nested functions with identical names), only one is tracked. The last one encountered wins. Qualified paths (e.g., `OuterClass.innerMethod`) are not built.
- **DSL limitations**: The DSL query must match nodes that have position information (`Pos` field) in the UAST. Nodes without position data cannot be mapped to diff hunks.
- **Large functions**: A change anywhere within a function's line range counts as a change to that function. Very large functions (hundreds of lines) will have inflated change counts.

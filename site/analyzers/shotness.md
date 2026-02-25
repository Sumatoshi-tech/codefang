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

### Coupling Strength

Coupling strength is normalized to a 0-1 scale using the formula:

```
strength(A, B) = co_changes(A, B) / max(co_changes(A, B), changes(A), changes(B))
```

This ensures the result is always in [0, 1] and provides a meaningful confidence metric. A strength of 1.0 means functions always change together; 0.5 means they co-change half the time relative to the most active function.

### Risk Classification

Nodes are classified into risk levels based on absolute change counts:

| Risk Level | Threshold | Meaning |
|---|---|---|
| **HIGH** | ≥ 20 changes | Requires immediate attention and robust test coverage |
| **MEDIUM** | ≥ 10 changes | Should be monitored and potentially refactored |
| **LOW** | < 10 changes | Normal change frequency |

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

## Output Formats

The shotness analyzer supports four output formats: JSON, YAML, text, and plot.

=== "Text"

    ```bash
    codefang run -a history/shotness -f text .
    ```

    Terminal output with color-coded sections:

    ```
    ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
    ┃ Shotness Analysis                              42 nodes   ┃
    ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

      Summary
      ──────────────────────────────────────────────────────────
      Total Nodes            42
      Total Changes          385
      Avg Changes/Node       9.2
      Total Couplings        156
      Avg Coupling Strength  34%
      Hot Nodes              8

      Hottest Functions
      ──────────────────────────────────────────────────────────
      processPayment (engine [████████████████████░] 1.0  (42 changes)
      validateInput (engine. [████████████████░░░░░] 0.8  (34 changes)

      Risk Assessment
      ──────────────────────────────────────────────────────────
      processPayment (engine HIGH    (42 changes)
      validateInput (engine. HIGH    (34 changes)

      Strongest Couplings
      ──────────────────────────────────────────────────────────
      processPayment ↔ validateInput   85%  (12 co-changes)
      handleRequest  ↔ parseBody       72%  (8 co-changes)
    ```

=== "JSON"

    ```bash
    codefang run -a history/shotness -f json .
    ```

    ```json
    {
      "node_hotness": [
        {
          "name": "processFile",
          "type": "Function",
          "file": "pkg/core/engine.go",
          "change_count": 42,
          "coupled_nodes": 3,
          "hotness_score": 1.0
        }
      ],
      "node_coupling": [
        {
          "node1_name": "processFile",
          "node1_file": "pkg/core/engine.go",
          "node2_name": "validate",
          "node2_file": "pkg/core/engine.go",
          "co_changes": 15,
          "coupling_strength": 0.36
        }
      ],
      "hotspot_nodes": [
        {
          "name": "processFile",
          "type": "Function",
          "file": "pkg/core/engine.go",
          "change_count": 42,
          "risk_level": "HIGH"
        }
      ],
      "aggregate": {
        "total_nodes": 3,
        "total_changes": 105,
        "total_couplings": 3,
        "avg_changes_per_node": 35.0,
        "avg_coupling_strength": 0.42,
        "hot_nodes": 2
      }
    }
    ```

=== "YAML"

    ```bash
    codefang run -a history/shotness -f yaml .
    ```

    ```yaml
    node_hotness:
      - name: processFile
        type: Function
        file: pkg/core/engine.go
        change_count: 42
        coupled_nodes: 3
        hotness_score: 1.0
    node_coupling:
      - node1_name: processFile
        node1_file: pkg/core/engine.go
        node2_name: validate
        node2_file: pkg/core/engine.go
        co_changes: 15
        coupling_strength: 0.36
    hotspot_nodes:
      - name: processFile
        type: Function
        file: pkg/core/engine.go
        change_count: 42
        risk_level: HIGH
    aggregate:
      total_nodes: 3
      total_changes: 105
      total_couplings: 3
      avg_changes_per_node: 35.0
      avg_coupling_strength: 0.42
      hot_nodes: 2
    ```

=== "Plot"

    ```bash
    codefang run -a history/shotness -f plot -o shotness.html .
    ```

    Generates an interactive HTML dashboard with three visualizations:

    1. **Code Hotness TreeMap**: Hierarchical file → function view sized by change frequency
    2. **Function Coupling Matrix**: Heatmap showing co-change frequency between functions
    3. **Top Hot Functions**: Bar chart comparing self-changes vs coupled changes

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

## Metrics Reference

### Node Hotness

| Field | Type | Description |
|---|---|---|
| `name` | string | Function/method name |
| `type` | string | UAST node type (e.g., "Function") |
| `file` | string | Source file path |
| `change_count` | int | Number of commits that modified this node |
| `coupled_nodes` | int | Number of other nodes that co-changed with this node |
| `hotness_score` | float | Normalized score [0, 1] relative to the hottest node |

### Node Coupling

| Field | Type | Description |
|---|---|---|
| `node1_name` / `node2_name` | string | Names of the coupled nodes |
| `node1_file` / `node2_file` | string | File paths of the coupled nodes |
| `co_changes` | int | Number of commits where both nodes changed |
| `coupling_strength` | float | Normalized strength [0, 1] |

### Aggregate

| Field | Type | Description |
|---|---|---|
| `total_nodes` | int | Total tracked nodes |
| `total_changes` | int | Sum of all node change counts |
| `total_couplings` | int | Number of unique coupling pairs |
| `avg_changes_per_node` | float | Mean changes per node |
| `avg_coupling_strength` | float | Mean coupling strength across all pairs |
| `hot_nodes` | int | Nodes with change count ≥ 10 (MEDIUM or HIGH risk) |

---

## Use Cases

- **Function-level hotspot detection**: Find the most frequently changed functions in the codebase. These are the highest-risk points for bugs.
- **Fine-grained coupling analysis**: Discover which functions always change together. This reveals implicit dependencies that file-level coupling misses.
- **Refactoring prioritization**: Functions that are both hot (high change count) and coupled (always change with others) are the best refactoring candidates.
- **Architecture validation**: Functions from different packages that are highly coupled may indicate a leaking abstraction.
- **Test prioritization**: Focus testing resources on the hottest functions.

---

## Interpreting Results

### Reading the Coupling Strength

| Strength | Interpretation |
|---|---|
| 0.8 - 1.0 | Very tight coupling. Functions almost always change together. Consider merging or extracting shared logic. |
| 0.5 - 0.8 | Moderate coupling. There is a significant shared dependency. Review if coupling is intentional. |
| 0.2 - 0.5 | Loose coupling. Occasional co-changes, likely due to shared APIs or data structures. |
| < 0.2 | Minimal coupling. Co-changes are incidental. |

### Actionable Insights

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

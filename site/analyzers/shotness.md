# Shotness analyzer reference

The shotness analyzer measures **structural hotness** -- the change frequency of
individual code entities (functions, methods, classes) across Git history, at
the UAST node level.

For the conceptual model — what node-level hotness and coupling mean and how
coupling strength is computed — see
[Understanding structural hotness](../explanation/shotness.md). To run it, see
the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

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

The `--shotness-dsl-struct` and `--shotness-dsl-name` CLI flags set these options for a single run.

### Custom DSL examples

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

## Output formats

The shotness analyzer supports four output formats: JSON, YAML, text, and plot.

=== "Text"

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

    Generates an interactive HTML dashboard with three visualizations:

    1. **Code Hotness TreeMap**: Hierarchical file → function view sized by change frequency
    2. **Function Coupling Matrix**: Heatmap showing co-change frequency between functions
    3. **Top Hot Functions**: Bar chart comparing self-changes vs coupled changes

---

## Metrics reference

### Node hotness

| Field | Type | Description |
|---|---|---|
| `name` | string | Function/method name |
| `type` | string | UAST node type (e.g., "Function") |
| `file` | string | Source file path |
| `change_count` | int | Number of commits that modified this node |
| `coupled_nodes` | int | Number of other nodes that co-changed with this node |
| `hotness_score` | float | Normalized score [0, 1] relative to the hottest node |

### Node coupling

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

## See also

- [Understanding structural hotness](../explanation/shotness.md) — the mental model, algorithm, architecture, interpretation, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

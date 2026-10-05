# Shotness analyzer reference

The shotness analyzer measures **structural hotness** -- the change frequency of
individual code entities (functions, methods, classes) across Git history, at
the UAST node level.

For the conceptual model — what node-level hotness and coupling mean and how
coupling strength is computed — see
[Understanding structural hotness](../explanation/shotness.md). To run it, see
the [Quick start](../getting-started/quickstart.md).

---

## What an entity is

An entity is a declaration the structural DSL selects **anywhere** in a
file's UAST — by default every node carrying the `Function` role, which
covers top-level functions, class methods (Java, C#, TypeScript, JavaScript,
Python, …), Go receiver methods and nested functions. Each entity is
identified by three things:

| Part | Source | Example |
|---|---|---|
| `type` | the UAST node type | `Function`, `Method`, `Class` |
| `name` | the **qualified** name: the names of the enclosing declarations, then the receiver type (Go-style methods), then the node's own name from the name DSL | `A.f`, `K.Inner.im`, `outer.inner`, `A.String` |
| `file` | the path at the commit being walked | `pkg/core/engine.go` |

Qualification uses the enclosing `Class`, `Struct`, `Interface`, `Enum`,
`Function` and `Method` declarations (a Java method `f` in class `A` is
`A.f`; a Python `def inner` inside `def outer` is `outer.inner`), and the
receiver type of a method declared as `func (a *A) String()` (`A.String`).
Two `String()` methods on two receivers are therefore two entities. A node
whose name DSL yields nothing (an anonymous lambda) is not an entity.

Rust `impl` methods are qualified only by their enclosing module-level
declarations: the mapping does not expose the `impl` target, so `fn m` in
`impl A` is the entity `m`. Mappings that set no `props.name` on
declarations (the real-world set in the
[shape contract](../architecture/uast.md#shape-contract) does) yield no
entities for that language.

## Entity lifecycle

The walk visits the commit window oldest-first and records, per commit, what
the commit did to each entity; every output format is a replay of those
records.

| Event | Effect |
|---|---|
| A file is added | every entity of the file is born with `change_count` 1 and coupled with the commit's other touched entities. |
| A file is modified | a diff hunk **touches** every entity whose line span it overlaps (deleted lines on the Before tree, inserted lines on the After tree); each touched entity counts **one** change for the commit, however many hunks hit it, and the commit's touched entities are coupled pairwise. |
| A declaration disappears from a file | the entity **ends**: it is dropped from the report together with every coupling that referenced it (with `--shotness-keep-ended` it stays, marked `"ended": true`, with its counts and couplings). Renaming a function in place (`f` → `f2`) ends `f` and starts `f2`. |
| A file is renamed (`git mv`, with or without edits) | every live entity of the file **moves** under the new path: counts and couplings are preserved and the old keys disappear. Ended entities keep the path they ended under. |
| A file is deleted | every entity of the file ends. |
| A merge commit | skipped: its diff against the first parent re-shows the side line that was already walked. Under `--first-parent` a merge is an ordinary commit and counts once. |

The report therefore lists exactly the entities that exist at the newest
commit of the window, each with the number of window commits that changed
it. An entity removed and later re-added under the same name is a new
entity.

`--shotness-keep-ended` (config key `history.shotness.keep_ended`) keeps
every entity the window saw instead: the ones that no longer exist are
marked `"ended": true` and keep their counts and couplings, the aggregate
adds `ended_nodes`, and an entity removed and declared again under the same
name and file is live again with its history. Hotspots and `hot_nodes`
count only live entities, and the plot pages show only live ones. On long
histories this grows the report: on 31,923 kubernetes commits the shotness
report goes from 256 MB to 859 MB (18,499 ended entities, couplings
1.06 M → 3.45 M).

Bounds shared with every history analyzer: blobs larger than 256 KiB are
not parsed (the file's entities are neither born nor touched by that
commit), and a commit whose raw tree diff exceeds `--max-changes-per-commit`
(default 10 000) is dropped before any analyzer sees it. A commit touching
more than 500 entities counts the touches but skips the pairwise coupling
update (a mass refactor is coupling noise).

---

## Configuration options

| Option | CLI flag | Default | Description |
|---|---|---|---|
| `Shotness.DSLStruct` | `--shotness-dsl-struct` | `rfilter(.roles has "Function")` | UAST DSL query selecting the tracked declarations, run against each parsed file. |
| `Shotness.DSLName` | `--shotness-dsl-name` | `.props.name` | UAST DSL query extracting a declaration's own name, run against each selected node and each enclosing declaration. |

Both expressions are compiled before the walk starts: an expression that
does not parse is a flag error (`invalid --shotness-dsl-struct "rfilter(":
DSL parse error: …`) and no report is produced. An empty value means the
default. The DSL grammar — `filter` over the direct children, `rfilter` /
`rmap` over the whole subtree, field access such as `.props.name` — is
documented under [`uast query`](../guide/cli-reference.md#uast-query).

The structural query must be recursive to see methods: `filter(...)` looks
at the file node's direct children only, so
`--shotness-dsl-struct 'filter(.roles has "Function")'` tracks top-level
functions and nothing inside a class.

### Custom DSL examples

=== "Track classes instead of functions"

    ```bash
    codefang run -a history/shotness --shotness-dsl-struct 'rfilter(.roles has "Class")' .
    ```

    Nested classes are qualified by their enclosing class (`K.Inner`).

=== "Track interfaces"

    ```bash
    codefang run -a history/shotness --shotness-dsl-struct 'rfilter(.roles has "Interface")' .
    ```

=== "Name entities by node type"

    ```bash
    codefang run -a history/shotness --shotness-dsl-name '.type' .
    ```

    Every selected node (and every enclosing declaration) is named by its
    type, so all methods of a Java class collapse into one `Class.Method`
    entity — a way to measure a whole class body as one unit.

Methods carry the `Function` role, so the default already tracks functions
and methods together.

---

## Output formats

The shotness analyzer supports json, yaml, text, ndjson, timeseries,
compact and plot output.

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
      Engine.processPayment  [████████████████████░] 1.0  (42 changes)
      Engine.validateInput ( [████████████████░░░░░] 0.8  (34 changes)

      Risk Assessment
      ──────────────────────────────────────────────────────────
      Engine.processPayment  HIGH    (42 changes)
      Engine.validateInput ( HIGH    (34 changes)

      Strongest Couplings
      ──────────────────────────────────────────────────────────
      Engine.processPayment ↔ Engine.validateInput   85%  (12 co-changes)
      handleRequest  ↔ parseBody       72%  (8 co-changes)
    ```

=== "JSON"

    ```json
    {
      "node_hotness": [
        {
          "name": "Engine.processFile",
          "type": "Method",
          "file": "pkg/core/engine.go",
          "change_count": 42,
          "coupled_nodes": 3,
          "hotness_score": 1.0
        }
      ],
      "node_coupling": [
        {
          "node1_name": "Engine.processFile",
          "node1_file": "pkg/core/engine.go",
          "node2_name": "validate",
          "node2_file": "pkg/core/engine.go",
          "co_changes": 15,
          "coupling_strength": 0.36
        }
      ],
      "hotspot_nodes": [
        {
          "name": "Engine.processFile",
          "type": "Method",
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
      - name: Engine.processFile
        type: Method
        file: pkg/core/engine.go
        change_count: 42
        coupled_nodes: 3
        hotness_score: 1.0
    node_coupling:
      - node1_name: Engine.processFile
        node1_file: pkg/core/engine.go
        node2_name: validate
        node2_file: pkg/core/engine.go
        co_changes: 15
        coupling_strength: 0.36
    hotspot_nodes:
      - name: Engine.processFile
        type: Method
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

=== "NDJSON / timeseries"

    `--format ndjson` emits one line per commit that touched an entity, with
    `data.NodesTouched` keyed by entity key (`Method_A.f_A.java`) and a
    `CountDelta` of 1 per touched entity. `--format timeseries` reports per
    commit `nodes_touched` and `coupling_pairs` (`C(n, 2)`). A pure `git mv`
    commit touches nothing and appears in neither.

=== "Plot"

    Generates an interactive HTML dashboard with three visualizations:

    1. **Code Hotness TreeMap**: Hierarchical file → entity view sized by change frequency
    2. **Function Coupling Matrix**: Heatmap showing co-change frequency between entities
    3. **Top Hot Functions**: Bar chart comparing self-changes vs coupled changes

---

## Metrics reference

### Node hotness

| Field | Type | Description |
|---|---|---|
| `name` | string | Qualified entity name (`Class.method`, `Receiver.method`) |
| `type` | string | UAST node type (e.g. `Function`, `Method`) |
| `file` | string | Source file path at the newest commit of the window |
| `change_count` | int | Number of commits that modified this entity |
| `coupled_nodes` | int | Number of other entities that co-changed with this entity |
| `hotness_score` | float | Normalized score [0, 1] relative to the hottest entity |
| `ended` | bool | Only with `--shotness-keep-ended`: the declaration (or its file) was removed and not declared again by the newest commit |

### Node coupling

| Field | Type | Description |
|---|---|---|
| `node1_name` / `node2_name` | string | Names of the coupled entities |
| `node1_file` / `node2_file` | string | File paths of the coupled entities |
| `co_changes` | int | Number of commits where both entities changed |
| `coupling_strength` | float | Normalized strength [0, 1] |

### Aggregate

| Field | Type | Description |
|---|---|---|
| `total_nodes` | int | Live entities at the end of the window (with `--shotness-keep-ended`: every entity the window saw) |
| `total_changes` | int | Sum of all entity change counts |
| `total_couplings` | int | Number of unique coupling pairs |
| `avg_changes_per_node` | float | Mean changes per entity |
| `avg_coupling_strength` | float | Mean coupling strength across all pairs |
| `hot_nodes` | int | Live entities with change count ≥ 10 (MEDIUM or HIGH risk) |
| `ended_nodes` | int | Only with `--shotness-keep-ended`: entities marked `ended` |

---

## See also

- [Understanding structural hotness](../explanation/shotness.md) — the mental model, algorithm, architecture, interpretation, and limitations.
- [`uast query`](../guide/cli-reference.md#uast-query) — the DSL the two options are written in.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

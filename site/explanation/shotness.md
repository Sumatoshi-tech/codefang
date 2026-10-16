# Understanding structural hotness

This page explains the mental model behind the shotness analyzer: what
node-level change frequency and coupling mean, how coupling strength is
computed, and how the walk is structured. For configuration keys, the entity
lifecycle table and the output schema, see the
[Shotness reference](../analyzers/shotness.md).

---

## What it measures

The shotness analyzer measures **structural hotness** -- the change frequency of
individual code entities (functions, methods, classes) across Git history.
Unlike the couples analyzer which operates at file granularity, shotness
operates at the UAST node level, providing fine-grained co-change analysis.

### Entity change frequency

For each declaration the structural DSL selects (every function-role node by
default — top-level functions, class methods, nested functions), the analyzer
counts how many commits modified lines within that entity's span. Entities
that change frequently are "hot" -- they are likely volatile, complex, or
central to the system. An entity is identified by its type, its **qualified
name** (`Class.method`, `Receiver.method`, `outer.inner`) and its file, so
same-named methods on different types never merge.

### Entity co-change coupling

When two entities are modified in the same commit, their coupling counter is
incremented. This produces a fine-grained coupling matrix at the entity
level, which is more precise than file-level coupling from the couples
analyzer.

### Coupling strength

Coupling strength is normalized to a 0-1 scale using the formula:

```text
strength(A, B) = co_changes(A, B) / max(co_changes(A, B), changes(A), changes(B))
```

This ensures the result is always in [0, 1] and provides a meaningful confidence metric. A strength of 1.0 means functions always change together; 0.5 means they co-change half the time relative to the most active function.

### Risk classification

Entities are classified into risk levels based on absolute change counts:

| Risk Level | Threshold | Meaning |
|---|---|---|
| **HIGH** | ≥ 20 changes | Requires immediate attention and robust test coverage |
| **MEDIUM** | ≥ 10 changes | Should be monitored and potentially refactored |
| **LOW** | < 10 changes | Normal change frequency |

### How it works

For each commit of the window, oldest first:

1. Compute the tree diff against the first parent (the whole tree for a
   root commit); drop the commit if the raw diff exceeds
   `--max-changes-per-commit`; apply the language / vendor / generated
   filter.
2. Parse the Before and After versions of each changed file into a UAST
   (blobs above 256 KiB are skipped).
3. Run the structural query (`dsl_struct`) over each tree to select the
   declarations, name each one with the name query (`dsl_name`) and qualify
   it with its enclosing declarations and receiver type.
4. Line-diff the two versions and map every hunk to the entities whose
   line span it overlaps.
5. Record the commit's **effects**: the entities it touched (one change
   each, coupled pairwise), the entities whose declaration or file
   disappeared (ended), and the entities a file rename moved.

Steps 1–4 are pure per-commit work and run in parallel across cores; step 5
runs sequentially in commit order over the live entity state, so the output
is a pure function of the repository and the flags, never of the host.
Merge commits are skipped (their diff re-shows the side line already
walked) unless the walk is `--first-parent`.

### Architecture

- **Pre-pass** (`cf-commands`, shared with the other UAST history
  analyzers): the tree diff, the per-commit parse cache and the entity
  extraction produce one state-independent *product* per change.
- **Reduce** (`cf-commands` → `cf-shotness`): the reducer turns each
  commit's products into `EntityEffect` records (touched / ended / moved
  from) over the live `ShotnessState`, resolving file-level events into the
  entities currently declared in the file.
- **Report** (`cf-shotness`): replaying the effect records in walk order
  over an empty state yields the live entity set; its key-sorted
  `Nodes`/`Counters` matrix feeds `compute_all_metrics` (hotness, coupling,
  hotspots, aggregate), the plot sections and the text report. The ndjson
  and timeseries formats list the same per-commit records.

There is one source of truth — the effect records and the state transition
that replays them — so a multi-analyzer run (which shares the walk with
imports, quality, sentiment and typos) and a standalone run produce the
same report.

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

- **UAST required**: Only languages with UAST parser support are analyzed. Files in unsupported languages, binary blobs, blobs above 256 KiB and files that fail to parse are skipped for that commit.
- **CPU intensive**: The analyzer parses both the before and after versions of every changed file in every commit. The per-commit work is spread across cores; the reduce is sequential.
- **The report is the present**: an entity that no longer exists at the newest commit of the window (its declaration was removed or its file deleted) is dropped, together with its couplings. An in-place rename of a declaration (`f` → `f2`) is an end plus a birth: the history of `f` does not carry over. A file rename does carry everything over. `--shotness-keep-ended` keeps the past instead: ended entities stay with their counts and couplings, marked `ended`, while hotspots and the plot pages still show only live entities.
- **Qualification follows the mapping**: Rust `impl` methods are not qualified by the `impl` target, and a language whose mapping sets no declaration names yields no entities. Two selected nodes in one file that resolve to the same type and qualified name are one entity.
- **Position required**: a selected node without position information cannot be mapped to diff hunks and is never touched.
- **Large entities**: A change anywhere within an entity's line range counts as a change to that entity. Very large functions (hundreds of lines) will have inflated change counts, and a change inside a method also touches the enclosing class when classes are selected.
- **Merge commits** are not counted (see the lifecycle table in the reference), so a side line's work is attributed to its own commits.

---

## See also

- [Shotness reference](../analyzers/shotness.md) — configuration keys, the entity lifecycle and the output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

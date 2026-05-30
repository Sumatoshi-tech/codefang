# Understanding code burndown

This page explains the mental model behind the burndown analyzer: what code
survival means, the per-file and per-developer breakdowns, and how the
line-tracking algorithm works. For configuration keys and the output schema,
see the [Burndown reference](../analyzers/burndown.md).

---

## What it measures

The burndown analyzer tracks **code survival over time** by following every
line of code through Git history. It produces burndown charts showing how code
written at different points in time persists, is modified, or is deleted.
Optionally, it tracks per-file and per-developer breakdowns.

### Global code survival

A time-series matrix where each row is a sampling point and each column is an age band. The value at `[sample][band]` is the number of lines that were last edited during that band and still survive at that sample point.

This matrix produces the classic **burndown chart**: stacked area plots showing how much code from each era remains.

### Per-file burndown

When `--burndown-files` is enabled, the analyzer produces a separate survival matrix for each file, enabling file-level burndown charts.

### Per-developer burndown

When `--burndown-people` is enabled, the analyzer tracks which developer last edited each line. This reveals:

- **Developer survival rates**: How much of each developer's code persists
- **Interaction matrix**: Which developers modify each other's code

### File ownership

When both `--burndown-files` and `--burndown-people` are enabled, the analyzer computes per-file ownership by iterating the live line segments in each file's internal tree. Each segment stores a packed `[author|tick]` value, from which the author ID is extracted to produce a `file -> author -> line_count` mapping.

!!! note "Ownership requires developer tracking"
    File ownership data is only available when `--burndown-people` is enabled. Without developer tracking, no author information is stored in the line segments, so file ownership will be empty.

!!! warning "Memory usage"
    Per-file and per-developer tracking significantly increases memory usage. For repositories with more than 100k commits, consider enabling hibernation (on by default).

---

## How it works

### Algorithm overview

1. **Commit traversal**: Commits are processed sequentially in topological order. For each commit, the analyzer diffs the parent tree against the current tree to find inserted, deleted, and modified lines.

2. **Line tracking**: Each line in every file is tracked using an augmented balanced binary tree (treap). Each tree node stores a contiguous range of lines with a packed value encoding `[author_id | creation_tick]`. When lines are inserted or deleted, the tree is split and merged to maintain the correct line-to-value mapping.

3. **Sparse history accumulation**: At each commit, the analyzer records deltas in a sparse history structure: `map[currentTick]map[creationTick]lineCountDelta`. This captures how many lines created at `creationTick` are alive at `currentTick`.

4. **Tick aggregation**: Sparse deltas from individual commits are accumulated into tick-level snapshots by a streaming aggregator. At each sampling boundary, the aggregator emits a `TickResult` containing the full sparse history state.

5. **Dense matrix conversion**: The sparse history is converted to a dense matrix (`DenseHistory`) via `groupSparseHistory()`. This involves:
    - **Tick normalization**: Map arbitrary tick values to sorted indices
    - **Granularity grouping**: Collapse adjacent creation ticks into age bands of width `granularity`
    - **Forward-fill**: For each age band, carry forward the last known value across sampling gaps to produce a complete matrix

6. **Metrics computation**: The dense matrix is used to compute aggregate statistics (survival rate, peak lines, current lines), per-developer survival, and the interaction matrix.

### Key parameters

- **Granularity** controls the width of each age band (in ticks). Higher values produce fewer, wider bands.
- **Sampling** controls how frequently snapshots are taken (in ticks). Higher values reduce the number of data points.
- **Tick size** defaults to a 24-hour window. All commits within the same day share one tick.

---

## Use cases

- **Project health monitoring**: Track the overall code survival rate. A declining rate may indicate churn or instability.
- **Developer contribution analysis**: Understand whose code persists and who rewrites existing code.
- **Code age visualization**: Generate burndown charts showing how much ancient code remains in the codebase.
- **Refactoring impact**: Measure how much code a refactoring effort actually replaced.
- **Team dynamics**: The interaction matrix reveals collaboration patterns -- who reviews and modifies whose code.

---

## Limitations

- **Sequential processing**: Burndown tracks cumulative per-line state across all commits and must process commits sequentially. It cannot be parallelized across commits (though per-file processing within a commit is parallelized via goroutines).
- **Memory intensive**: Every line in every file is tracked throughout history. Large repositories (100k+ commits, 10k+ files) can require several GB of RAM. Use hibernation options to manage memory.
- **Binary files excluded**: Binary files are automatically skipped since they cannot be meaningfully diff'd line-by-line.
- **Rename tracking**: File renames are tracked using Git's rename detection. If Git does not detect a rename (e.g., content changed significantly), the file appears as a deletion + insertion.
- **Tick resolution**: The default 24-hour tick means that all commits within the same day share one tick. Sub-day granularity is not supported.
- **File ownership without developer tracking**: File ownership data requires `--burndown-people`. Without it, the `file_survival` array will have empty ownership maps.

---

## See also

- [Burndown reference](../analyzers/burndown.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

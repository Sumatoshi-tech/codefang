# Understanding file and developer coupling

This page explains the mental model behind the couples analyzer: what file and
developer coupling mean, how coupling strength is computed, and how the
aggregation pipeline is structured. For configuration keys and the output
schema, see the [Couples reference](../analyzers/couples.md).

---

## What it measures

The couples analyzer identifies **file coupling** and **developer coupling** by
analyzing co-change patterns across Git history. Files that frequently change
together are likely coupled; developers who frequently touch the same files are
collaborating (or competing).

### File coupling

A square matrix where each cell `[i][j]` counts the number of commits in which files `i` and `j` both appeared. High co-change counts indicate tight coupling between files.

**Coupling strength** is computed using the code-maat formula:

```text
strength = co_changes / average(revisions_file1, revisions_file2)
```

Where `revisions_fileN` is the diagonal element of the file matrix (the file's self-change count). The result is capped at 1.0. This normalizes coupling by the average activity of both files, so a pair of files that change together 10 times out of 20 average revisions scores 50%.

!!! info "Changeset size filter"
    Commits touching more than **1000 files** are excluded from coupling analysis. Mass changes (e.g., formatting, license updates, dependency bumps) produce noise rather than meaningful coupling signal.

### Developer coupling

A matrix where each cell `[i][j]` counts the number of times developers `i` and `j` committed to the same file. Developer coupling strength uses the same code-maat formula normalized by each developer's individual commit activity.

### File ownership

For each tracked file, the analyzer reports its line count and the number of distinct contributors. Single-owner files are flagged as bus-factor risks.

### Rename tracking

The analyzer tracks file renames to avoid counting a renamed file as both a deletion and an insertion, which would break coupling chains.

---

## Use cases

- **Architecture analysis**: Highly coupled file pairs that span package boundaries may indicate architectural violations.
- **Dependency discovery**: Find implicit dependencies that are not captured by import statements. If two files always change together, they are coupled even without explicit imports.
- **Module boundary validation**: Files within the same module should be more coupled than files across modules. Cross-module coupling is a design smell.
- **Team topology**: Developer coupling reveals who collaborates with whom. This can inform team structure decisions.
- **Change impact prediction**: When modifying a file, the coupling matrix predicts which other files are likely to need changes.
- **Bus factor assessment**: File ownership data identifies single-owner files that represent knowledge concentration risks.

---

## Architecture

The couples analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: Each commit produces a `TC{Data: *CommitData}` containing the coupling context (list of co-changed files), per-file author touch counts (always 1 per file per commit), rename pairs, and whether the author's commit count was incremented. Commits exceeding `CouplesMaximumMeaningfulContextSize` (1000 files) are skipped, producing an empty coupling context.

2. **Aggregation phase**: The `Aggregator` accumulates the file co-occurrence matrix using `SpillStore[map[string]int]`, along with per-person file touch counts (`people`), commit counts (`peopleCommits`), and rename tracking. When memory pressure exceeds `SpillBudget`, the file coupling matrix is spilled to disk via gob encoding. `Collect()` merges spilled data using additive merge semantics.

3. **Serialization phase**: `ticksToReport()` reconstructs the full report (PeopleMatrix, PeopleFiles, Files, FilesLines, FilesMatrix, ReversedPeopleDict) from aggregated TICKs, then `ComputeAllMetrics()` produces the final typed output for any format (text, plot, JSON, YAML, binary).

**Working state** (`merges`, `seenFiles`) stays in the analyzer for merge-mode dedup across commits. **Accumulated output** (file couplings, people maps, renames) is owned entirely by the aggregator.

---

## Methodology

The coupling strength formula follows the **code-maat** academic standard (Adam Tornhill, "Your Code as a Crime Scene"):

```text
degree = shared_revisions / average(revisions_A, revisions_B)
```

This measures what fraction of a file pair's average activity is shared. A coupling strength of 80% means the pair changes together in 80% of their average revision count.

The aggregate `avg_coupling_strength` is the mean of all per-pair coupling strengths (not the mean of raw co-change counts).

**Highly coupled pairs** are those with 10 or more co-changes (raw count threshold).

---

## Limitations

- **Large commits excluded**: Commits touching more than 1000 files are skipped to avoid noise from mass changes (formatting, license headers, dependency updates).
- **No temporal decay**: All commits are weighted equally. A coupling from long ago counts the same as a recent one. Consider filtering by date range for recent coupling analysis.
- **Merge commits**: Merge commits are processed only once (first encounter) to avoid double-counting changes that appear in multiple branches.
- **File deletions**: Deleted files are included in the coupling matrix during the analysis window but may not appear in the final output if they no longer exist at HEAD.

---

## See also

- [Couples reference](../analyzers/couples.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

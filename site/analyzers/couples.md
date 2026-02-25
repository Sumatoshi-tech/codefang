# Couples Analyzer

The couples analyzer identifies **file coupling** and **developer coupling** by analyzing co-change patterns across Git history. Files that frequently change together are likely coupled; developers who frequently touch the same files are collaborating (or competing).

---

## Quick Start

```bash
codefang run -a history/couples .
```

---

## What It Measures

### File Coupling

A square matrix where each cell `[i][j]` counts the number of commits in which files `i` and `j` both appeared. High co-change counts indicate tight coupling between files.

**Coupling strength** is computed using the code-maat formula:

```
strength = co_changes / average(revisions_file1, revisions_file2)
```

Where `revisions_fileN` is the diagonal element of the file matrix (the file's self-change count). The result is capped at 1.0. This normalizes coupling by the average activity of both files, so a pair of files that change together 10 times out of 20 average revisions scores 50%.

!!! info "Changeset size filter"
    Commits touching more than **1000 files** are excluded from coupling analysis. Mass changes (e.g., formatting, license updates, dependency bumps) produce noise rather than meaningful coupling signal.

### Developer Coupling

A matrix where each cell `[i][j]` counts the number of times developers `i` and `j` committed to the same file. Developer coupling strength uses the same code-maat formula normalized by each developer's individual commit activity.

### File Ownership

For each tracked file, the analyzer reports its line count and the number of distinct contributors. Single-owner files are flagged as bus-factor risks.

### Rename Tracking

The analyzer tracks file renames to avoid counting a renamed file as both a deletion and an insertion, which would break coupling chains.

---

## Configuration Options

The couples analyzer has no additional configuration options. It uses the identity detector for developer mapping and the tree diff analyzer for change detection.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific configuration |

---

## Output Formats

### Text

The text format provides a terminal-friendly summary with:

- **Summary**: Total files, developers, co-changes, highly coupled pairs, average coupling strength
- **Top File Couples**: Top 7 most coupled file pairs with co-change count and strength percentage
- **Top Developer Couples**: Top 7 developer pairs with shared file changes and coupling strength
- **File Ownership Risk**: Files sorted by fewest contributors (highest bus-factor risk first), with single-contributor files flagged

### Plot (HTML)

The HTML plot includes three interactive chart sections:

1. **Top File Couples** — Horizontal bar chart of the 20 most co-changed file pairs
2. **Developer Coupling Heatmap** — Matrix showing developer collaboration intensity
3. **File Ownership Distribution** — Pie chart categorizing files by contributor count (single owner, 2-3, 4-5, 6+)

### JSON / YAML

Structured output with four top-level sections: `file_coupling`, `developer_coupling`, `file_ownership`, and `aggregate`.

### Executive Summary (ReportSection)

The couples analyzer provides a `ReportSection` for use in combined reports:

- **Score**: `1.0 - avg_coupling_strength` (lower coupling = better score, 0-1 scale)
- **Key Metrics**: Total files, developers, co-changes, highly coupled pairs, average coupling
- **Distribution**: Coupling strength buckets — Strong (>70%), Moderate (40-70%), Weak (10-40%), Minimal (<10%)
- **Issues**: File pairs sorted by coupling strength descending, with severity labels (poor/fair/good)

---

## Example Output

=== "JSON"

    ```json
    {
      "file_coupling": [
        {
          "file1": "pkg/core/engine.go",
          "file2": "pkg/core/engine_test.go",
          "co_changes": 87,
          "coupling_strength": 0.92
        },
        {
          "file1": "pkg/api/handler.go",
          "file2": "pkg/api/routes.go",
          "co_changes": 45,
          "coupling_strength": 0.78
        }
      ],
      "developer_coupling": [
        {
          "developer1": "alice",
          "developer2": "bob",
          "shared_file_changes": 234,
          "coupling_strength": 0.65
        }
      ],
      "file_ownership": [
        {
          "file": "pkg/core/engine.go",
          "lines": 450,
          "contributors": 3
        }
      ],
      "aggregate": {
        "total_files": 342,
        "total_developers": 5,
        "total_co_changes": 12500,
        "avg_coupling_strength": 0.42,
        "highly_coupled_pairs": 23
      }
    }
    ```

=== "YAML"

    ```yaml
    file_coupling:
      - file1: pkg/core/engine.go
        file2: pkg/core/engine_test.go
        co_changes: 87
        coupling_strength: 0.92
    developer_coupling:
      - developer1: alice
        developer2: bob
        shared_file_changes: 234
        coupling_strength: 0.65
    file_ownership:
      - file: pkg/core/engine.go
        lines: 450
        contributors: 3
    aggregate:
      total_files: 342
      total_developers: 5
      total_co_changes: 12500
      avg_coupling_strength: 0.42
      highly_coupled_pairs: 23
    ```

---

## Use Cases

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

```
degree = shared_revisions / average(revisions_A, revisions_B)
```

This measures what fraction of a file pair's average activity is shared. A coupling strength of 80% means the pair changes together in 80% of their average revision count.

The aggregate `avg_coupling_strength` is the mean of all per-pair coupling strengths (not the mean of raw co-change counts).

**Highly coupled pairs** are those with 10 or more co-changes (raw count threshold).

---

## Limitations

- **Large commits excluded**: Commits touching more than 1000 files are skipped to avoid noise from mass changes (formatting, license headers, dependency updates).
- **No temporal decay**: All commits are weighted equally. A coupling from three years ago counts the same as one from yesterday. Consider filtering by date range for recent coupling analysis.
- **Merge commits**: Merge commits are processed only once (first encounter) to avoid double-counting changes that appear in multiple branches.
- **File deletions**: Deleted files are included in the coupling matrix during the analysis window but may not appear in the final output if they no longer exist at HEAD.

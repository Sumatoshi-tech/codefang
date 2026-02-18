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

!!! info "Meaningful context size"
    Commits touching more than **1000 files** are excluded from coupling analysis, since mass changes (e.g., formatting, license updates) produce noise rather than signal.

### Developer Coupling

A matrix where each cell `[i][j]` counts the number of times developers `i` and `j` committed to the same file. This reveals collaboration patterns and shared code ownership.

### File Ownership

For each tracked file, the analyzer reports its line count and the number of distinct contributors.

### Rename Tracking

The analyzer tracks file renames to avoid counting a renamed file as both a deletion and an insertion, which would break coupling chains.

---

## Configuration Options

The couples analyzer has no additional configuration options. It uses the identity detector for developer mapping and the tree diff analyzer for change detection.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific configuration |

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
        "avg_coupling_strength": 4.2,
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
    aggregate:
      total_files: 342
      total_developers: 5
      highly_coupled_pairs: 23
    ```

---

## Use Cases

- **Architecture analysis**: Highly coupled file pairs that span package boundaries may indicate architectural violations.
- **Dependency discovery**: Find implicit dependencies that are not captured by import statements. If two files always change together, they are coupled even without explicit imports.
- **Module boundary validation**: Files within the same module should be more coupled than files across modules. Cross-module coupling is a design smell.
- **Team topology**: Developer coupling reveals who collaborates with whom. This can inform team structure decisions.
- **Change impact prediction**: When modifying a file, the coupling matrix predicts which other files are likely to need changes.

---

## Limitations

- **Large commits excluded**: Commits touching more than 1000 files are ignored to avoid noise from mass changes (formatting, license headers, dependency updates).
- **Coupling strength**: The coupling strength metric is normalized by the maximum co-change count for the pair. It does not account for the total number of commits each file has individually.
- **No temporal decay**: All commits are weighted equally. A coupling from three years ago counts the same as one from yesterday. Consider filtering by date range for recent coupling analysis.
- **Merge commits**: Merge commits are processed only once (first encounter) to avoid double-counting changes that appear in multiple branches.
- **File deletions**: Deleted files are included in the coupling matrix during the analysis window but may not appear in the final output if they no longer exist at HEAD.

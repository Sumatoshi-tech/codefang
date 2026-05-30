# Couples analyzer reference

The couples analyzer identifies **file coupling** and **developer coupling** by
analyzing co-change patterns across Git history.

For the conceptual model — what coupling means, how coupling strength is
computed, and the methodology — see
[Understanding file and developer coupling](../explanation/couples.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The couples analyzer has no additional configuration options. It uses the identity detector for developer mapping and the tree diff analyzer for change detection.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific configuration |

---

## Output formats

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

### Executive summary (ReportSection)

The couples analyzer provides a `ReportSection` for use in combined reports:

- **Score**: `1.0 - avg_coupling_strength` (lower coupling = better score, 0-1 scale)
- **Key Metrics**: Total files, developers, co-changes, highly coupled pairs, average coupling
- **Distribution**: Coupling strength buckets — Strong (>70%), Moderate (40-70%), Weak (10-40%), Minimal (<10%)
- **Issues**: File pairs sorted by coupling strength descending, with severity labels (poor/fair/good)

---

## Example output

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
          "developer1_email": "alice@example.com",
          "developer2": "bob",
          "developer2_email": "bob@example.com",
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

## See also

- [Understanding file and developer coupling](../explanation/couples.md) — the mental model, architecture, methodology, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

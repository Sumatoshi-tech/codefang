# Couples analyzer reference

The couples analyzer identifies **file coupling** and **developer coupling** by
analyzing co-change patterns across Git history.

For the conceptual model — what coupling means, how coupling strength is
computed, and how renames, merges and large commits are handled — see
[Understanding file and developer coupling](../explanation/couples.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The couples analyzer has no additional configuration options. It uses the
shared history core: the commit window (`--limit`, `--since`, `--head`,
`--first-parent`), the identity detector (`--exact-signatures`,
`--people-dict`, `--anonymize`) and the change filter (`--languages`,
vendor / generated policy) documented in the
[CLI reference](../guide/cli-reference.md).

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific configuration |

The changeset cap is fixed: a commit touching more than **1000 files** feeds
no coupling context (see the explanation page).

---

## Output formats

### Text

The text format provides a terminal-friendly summary with:

- **Summary**: Total files, developers, co-changes, highly coupled pairs, average coupling strength
- **Top File Couples**: Top 7 most coupled file pairs with co-change count and strength percentage
- **Top Developer Couples**: Top 7 developer pairs with shared file changes and coupling strength
- **File Ownership Risk**: Files sorted by fewest contributors (highest bus-factor risk first), with single-contributor files flagged

### Plot (HTML)

The HTML plot includes two interactive chart sections:

1. **Top File Couples** — Horizontal bar chart of the 20 most co-changed file pairs
2. **File Ownership Distribution** — Pie chart categorizing files by contributor count (single owner, 2-3, 4-5, 6+)

### JSON / YAML

Structured output with four top-level sections: `file_coupling`,
`developer_coupling`, `file_ownership`, and `aggregate`.

| Section | Field | Meaning |
|---|---|---|
| `file_coupling[]` | `co_changes` | Number of commits in the window in which both files changed |
| `file_coupling[]` | `coupling_strength` | `co_changes / avg(changes_file1, changes_file2)`, capped at 1.0 |
| `developer_coupling[]` | `shared_file_changes` | Sum over shared files of `min(changes_dev1, changes_dev2)` |
| `file_ownership[]` | `lines` | Newline count of the file at the window head |
| `file_ownership[]` | `contributors` | Distinct developers who changed the file in the window |
| `aggregate` | `total_files` | Files present at the window head that were changed in the window |
| `aggregate` | `avg_coupling_strength` | Mean of the per-pair coupling strengths |
| `aggregate` | `highly_coupled_pairs` | Pairs with 10 or more co-changes |

`file_coupling` is `null` (YAML `[]`) when no pair co-changed in the window.

### NDJSON / timeseries

`--format ndjson` emits one record per walked commit with
`CouplingFiles` (the coupling context — empty above the cap), `AuthorFiles`
(every file the author touched, deletions included), `Renames`
(`FromName` / `ToName` pairs) and `CommitCounted`. `--format timeseries`
reports, per commit and under its tick, `files_touched` (the size of
`AuthorFiles`) and `author_id`.

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

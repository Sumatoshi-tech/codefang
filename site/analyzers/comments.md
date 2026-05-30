# Comments analyzer reference

The comments analyzer evaluates **documentation coverage** by examining comment
presence, placement, and density across your source code.

For the conceptual model — what documentation coverage means and what a healthy
comment density looks like — see
[Understanding documentation coverage](../explanation/comments.md). To run it,
see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The comments analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example output

=== "JSON"

    ```json
    {
      "comments": {
        "files": [
          {
            "file": "main.go",
            "total_lines": 250,
            "comment_lines": 45,
            "comment_density": 0.18,
            "functions_total": 12,
            "functions_documented": 8,
            "documentation_coverage": 0.67,
            "undocumented": [
              {"name": "helperFunc", "line": 89},
              {"name": "processItem", "line": 134},
              {"name": "cleanup", "line": 201},
              {"name": "retryLoop", "line": 220}
            ]
          }
        ],
        "summary": {
          "total_files": 1,
          "avg_comment_density": 0.18,
          "avg_documentation_coverage": 0.67,
          "total_undocumented_functions": 4
        }
      }
    }
    ```

=== "Text"

    ```
    Documentation Coverage
      main.go
        density=18%  coverage=67% (8/12 functions documented)
        undocumented: helperFunc:89 processItem:134 cleanup:201 retryLoop:220

    Summary: 1 file, avg density=18%, avg coverage=67%
    ```

---

## See also

- [Understanding documentation coverage](../explanation/comments.md) — the mental model, comment types, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

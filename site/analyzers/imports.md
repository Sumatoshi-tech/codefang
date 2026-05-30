# Imports analyzer reference

The imports analyzer extracts **import and dependency information** from source
code using UAST parsing. It operates in both static mode (single-file analysis)
and history mode (tracking import usage per developer over time).

For the conceptual model — what it measures and how the history-mode pipeline
works — see [Understanding import and dependency
analysis](../explanation/imports.md). To run it, see the
[Quick start](../getting-started/quickstart.md).

---

## Configuration options

### Static mode

No configuration options. Uses UAST directly.

### History mode

| Option | Type | Default | Description |
|---|---|---|---|
| `Imports.Goroutines` | `int` | `4` | Number of parallel goroutines for import extraction |
| `Imports.MaxFileSize` | `int` | `1048576` | Maximum file size in bytes; larger files are skipped |

Set options via the configuration file:

```yaml
# .codefang.yml
history:
  imports:
    goroutines: 8
    max_file_size: 2097152  # 2 MB
```

---

## Example output

=== "Static (JSON)"

    ```json
    {
      "imports": {
        "files": [
          {
            "file": "main.go",
            "language": "Go",
            "imports": [
              "fmt",
              "os",
              "encoding/json",
              "github.com/example/lib"
            ]
          }
        ]
      }
    }
    ```

=== "History (YAML)"

    ```yaml
    tick_size: 86400
    imports:
      alice: {"Go":{"fmt":{"0":5,"1":3},"net/http":{"2":1}}}
      bob: {"Go":{"encoding/json":{"0":2,"3":4}}}
    ```

    Each entry maps `developer -> language -> import_path -> tick -> count`.

---

## See also

- [Understanding import and dependency analysis](../explanation/imports.md) — the mental model, history-mode architecture, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static and history analysis.

# Imports Analyzer

The imports analyzer extracts **import and dependency information** from source code using UAST parsing. It operates in both static mode (single-file analysis) and history mode (tracking import usage per developer over time).

---

## Quick Start

=== "Static mode"

    ```bash
    uast parse main.go | codefang analyze -a imports
    ```

=== "History mode"

    ```bash
    codefang run -a history/imports .
    ```

---

## Architecture (History Mode)

The imports history analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: For each commit, `Consume()` extracts imports from changed files via parallel UAST parsing and returns them as `TC{Data: []ImportEntry}`. Each `ImportEntry` carries a language and import path. The analyzer retains no per-commit state; only the UAST parser is kept as working state.
2. **Aggregation phase**: An `imports.Aggregator` collects TCs into a 4-level `Map` (author -> language -> import -> tick -> count) using `SpillStore[Map]`. The `AuthorID` and `Tick` from each TC index the entries correctly.
3. **Serialization phase**: `SerializeTICKs()` merges all tick data back into the full `Map` with metadata (`author_index`, `tick_size`), then delegates to `Serialize()` for JSON, YAML, binary, or HTML plot output.

This separation enables streaming output, budget-aware memory spilling, and decoupled aggregation.

---

## What It Measures

### Static Mode

- **Import list**: All imports/dependencies declared in each file
- **Language detection**: Automatically detects the language and normalizes import paths
- **Dependency graph**: Maps which files depend on which packages

### History Mode

Tracks import usage across Git history, producing a per-developer, per-language, per-tick breakdown of which dependencies each developer introduces or modifies. This reveals:

- **Dependency adoption timeline**: When new libraries were introduced
- **Developer expertise signals**: Which developers work with which dependencies
- **Technology spread**: How quickly new dependencies propagate across the team

---

## Configuration Options

### Static Mode

No configuration options. Uses UAST directly.

### History Mode

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

## Example Output

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

## Use Cases

- **Dependency auditing**: List all third-party dependencies used in a project.
- **Developer profiling**: Understand which developers work with which frameworks and libraries.
- **Technology adoption tracking**: Monitor when and how quickly new dependencies spread across the team.
- **License compliance**: Extract the full dependency list for license scanning pipelines.
- **Architecture enforcement**: Detect unauthorized imports from forbidden packages.

---

## Limitations

- **Language support**: Only languages with UAST parser support are analyzed. Unsupported file types are silently skipped.
- **Dynamic imports**: Runtime or dynamic imports (e.g., Python's `importlib.import_module()`, JavaScript's `import()`) are not detected since they are not present in the static UAST.
- **Transitive dependencies**: Only direct imports are reported. The analyzer does not resolve transitive dependency trees.
- **File size threshold**: Files exceeding `MaxFileSize` (default 1 MB) are skipped to avoid excessive memory usage during parallel extraction.
- **History mode overhead**: History mode creates a UAST parser per fork, which increases memory usage. Tune `Goroutines` based on available memory.

# Understanding import and dependency analysis

This page explains the mental model behind the imports analyzer: what it
measures in static and history modes, and how the history-mode aggregation
pipeline works. For configuration keys and the output schema, see the
[Imports reference](../analyzers/imports.md).

---

## What it measures

The imports analyzer extracts **import and dependency information** from source
code using UAST parsing. It operates in both static mode (single-file analysis)
and history mode (tracking import usage per developer over time).

### Static mode

- **Import list**: All imports/dependencies declared in each file
- **Language detection**: Automatically detects the language and normalizes import paths
- **Dependency graph**: Maps which files depend on which packages

### History mode

Tracks import usage across Git history, producing a per-developer, per-language, per-tick breakdown of which dependencies each developer introduces or modifies. This reveals:

- **Dependency adoption timeline**: When new libraries were introduced
- **Developer expertise signals**: Which developers work with which dependencies
- **Technology spread**: How quickly new dependencies propagate across the team

---

## Architecture (history mode)

The imports history analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: For each commit, `Consume()` extracts imports from changed files via parallel UAST parsing and returns them as `TC{Data: []ImportEntry}`. Each `ImportEntry` carries a language and import path. The analyzer retains no per-commit state; only the UAST parser is kept as working state.
2. **Aggregation phase**: An `imports.Aggregator` collects TCs into a 4-level `Map` (author -> language -> import -> tick -> count) using `SpillStore[Map]`. The `AuthorID` and `Tick` from each TC index the entries correctly.
3. **Serialization phase**: `SerializeTICKs()` merges all tick data back into the full `Map` with metadata (`author_index`, `tick_size`), then delegates to `Serialize()` for JSON, YAML, binary, or HTML plot output.

This separation enables streaming output, budget-aware memory spilling, and decoupled aggregation.

---

## Use cases

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

---

## See also

- [Imports reference](../analyzers/imports.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

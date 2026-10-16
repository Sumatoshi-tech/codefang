# Understanding import and dependency analysis

This page explains the mental model behind the imports analyzer: what it
measures in static and history modes, and how the history-mode aggregation
pipeline works. For the per-language extraction rules, the classification
table and the output schema, see the [Imports reference](../analyzers/imports.md).

---

## What it measures

The imports analyzer extracts **import and dependency information** from source
code using UAST parsing. It operates in both static mode (per-file analysis
aggregated over a tree) and history mode (tracking import usage per developer
over time).

### Static mode

- **Import targets, not statements**: each file contributes the modules,
  packages or files its import statements name — `strings` for Go's
  `str "strings"`, `react` for `import { useState } from 'react'`,
  `std::collections::HashMap` and `std::collections::HashSet` for one Rust
  `use` with a brace group. Aliases, specifier lists and keywords are never
  imports. Each target counts once per file.
- **Usage across files**: the aggregate reports every unique import with the
  number of files importing it, so the most-shared dependencies surface
  first.
- **Classification by the importing file's language**: `relative`,
  `stdlib` or `external`, using per-language standard-library tables (a Go
  `errors` and a Python `datetime` are both stdlib; a JavaScript `react` is
  external).

### History mode

Tracks import usage across Git history, producing a per-developer, per-language, per-tick breakdown of which dependencies each developer introduces or modifies: every commit's changed files (the After version of each Insert/Modify) are parsed, their imports are extracted with the file's language, and each import is counted under the commit's author, the file's language and the commit's tick. This reveals:

- **Dependency adoption timeline**: When new libraries were introduced
- **Developer expertise signals**: Which developers work with which dependencies
- **Technology spread**: How quickly new dependencies propagate across the team

---

## How extraction works

The extractor walks the parsed tree with one rule per language (Go, Python,
JavaScript/TypeScript, Rust, Java, C/C++, C#, PHP, Ruby, Kotlin) and a
generic rule for everything else. A rule recognizes an import node by its
shape — a statement token to parse, a quoted literal child, an identifier
child, or a `require`/`import` call — and once a node matches, its subtree
is never visited again. That consuming walk is what keeps nested
specifier nodes (`React`, `{ useState, useEffect }`, `* as ns`) out of the
result.

---

## Architecture (history mode)

The map and its report live in the `cf-imports` crate; the history walk in `cf-commands` supplies the per-commit inputs:

1. **Per-commit product**: for every Insert/Modify change the walk parses the After blob (files the parser supports, up to 256 KiB) and runs `cf_imports::extract_imports` with the parser's language for the file and the blob bytes — the same extractor and per-language rules as `static/imports`. The product is the commit's list of `(language, import)` entries, a pure function of the commit, computed in parallel across cores.
2. **Chronological reduce**: the shared history core stamps every commit with its author id and tick in oldest-first order; `add_entries_to_map` folds each commit's entries into the 4-level map `author -> language -> import -> tick -> count`, which merges additively.
3. **Report**: `cf_imports::HistoryReport` carries the tick size, the author index and the map keyed by developer label; its one value tree is what json, yaml and bin encode, and `aggregate_import_counts` / `top_imports` derive the plot's top-20 bars from the same map.

---

## Use cases

- **Dependency auditing**: List all third-party dependencies used in a project.
- **Developer profiling**: Understand which developers work with which frameworks and libraries.
- **Technology adoption tracking**: Monitor when and how quickly new dependencies spread across the team.
- **License compliance**: Extract the full dependency list for license scanning pipelines.
- **Architecture enforcement**: Detect unauthorized imports from forbidden packages.

---

## Limitations

- **Language support**: Only languages with UAST parser support are analyzed. Unsupported file types are silently skipped. Languages without a dedicated rule use the generic statement rule, which covers the common `import` / `from … import` / `use` forms.
- **Dynamic imports**: JavaScript `import('m')` and `require('m')` are detected only when the argument is a string literal; computed module names (`require(name)`) and Python's `importlib.import_module()` are not present as imports in the UAST.
- **Re-exports**: `export { x } from './m'` is not an import.
- **Kotlin**: the Kotlin grammar lowers imports without tokens, so the statement is read back from the file bytes; both the static and the history path provide them.
- **Transitive dependencies**: Only direct imports are reported. The analyzer does not resolve transitive dependency trees.
- **Blob size threshold**: In history mode, blobs above 256 KiB are not parsed.
- **Counts are per change, not per file**: history mode counts the imports of every Insert/Modify change's After version, so a file rewritten in several commits contributes its imports once per commit.
- **History mode overhead**: Every changed file of every commit is parsed; one UAST parser per worker thread is reused across the commits that thread processes.

---

## See also

- [Imports reference](../analyzers/imports.md) — extraction rules, classification, configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

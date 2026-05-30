# Code quality analyzer reference

The code quality analyzer tracks **complexity, Halstead metrics, comment
quality, and cohesion** across Git history. For each commit, it runs four
static analyzers on UAST-parsed changed files and produces per-file quality
metrics.

For the conceptual model — which metrics it tracks and how it aggregates them
per tick — see [Understanding code quality over
time](../explanation/quality.md). To run it, see the
[Quick start](../getting-started/quickstart.md).

---

## Configuration options

The quality analyzer has no configurable options. It uses the UAST pipeline's language detection and parsing configuration.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Output formats

| Format | Flag | Description |
|---|---|---|
| JSON | `--format json` | `ComputedMetrics` with `time_series` and `aggregate` fields |
| YAML | `--format yaml` | Same structure as JSON |
| Plot | `--format plot` | HTML page with complexity and Halstead charts plus summary stats |
| Binary | `--format binary` | Compact binary envelope for programmatic consumption |

---

## See also

- [Understanding code quality over time](../explanation/quality.md) — the mental model, per-file metrics, and architecture.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

# Complexity analyzer reference

The complexity analyzer measures three dimensions of code complexity from
source code: **cyclomatic complexity**, **cognitive complexity**, and
**nesting depth**. It operates on the UAST representation of your source
files.

For the conceptual model — what each metric means and how cyclomatic and
cognitive complexity differ — see
[Understanding complexity metrics](../explanation/complexity.md). To run it,
see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The complexity analyzer uses the UAST directly and has no analyzer-specific configuration options. Language support is determined by the UAST parser.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example output

=== "JSON"

    ```json
    {
      "function_complexity": [
        {
          "name": "processFile",
          "source_file": "cmd/server/main.go",
          "language": "go",
          "directory": "cmd/server",
          "cyclomatic_complexity": 8,
          "cognitive_complexity": 12,
          "nesting_depth": 3,
          "lines_of_code": 45,
          "complexity_density": 0.178,
          "risk_level": "LOW"
        },
        {
          "name": "validate",
          "source_file": "cmd/server/main.go",
          "language": "go",
          "directory": "cmd/server",
          "cyclomatic_complexity": 15,
          "cognitive_complexity": 22,
          "nesting_depth": 5,
          "lines_of_code": 80,
          "complexity_density": 0.188,
          "risk_level": "MEDIUM"
        }
      ],
      "high_risk_functions": [
        {
          "name": "validate",
          "source_file": "cmd/server/main.go",
          "language": "go",
          "directory": "cmd/server",
          "cyclomatic_complexity": 15,
          "cognitive_complexity": 22,
          "risk_level": "MEDIUM",
          "issues": ["High cyclomatic complexity", "Deep nesting"]
        }
      ],
      "distribution": {
        "simple": 180,
        "moderate": 25,
        "complex": 7
      },
      "aggregate": {
        "total_functions": 212,
        "average_complexity": 3.2,
        "max_complexity": 15,
        "health_score": 78.5,
        "message": "Fair complexity - some functions could be simplified"
      }
    }
    ```

    Each function record includes `source_file`, `language`, and `directory`
    for file-level joins and DWH aggregation.

=== "Text"

    ```
    Complexity Analysis
      processFile  (main.go:42)   cyclomatic=8   cognitive=12  nesting=3
      validate     (main.go:105)  cyclomatic=15  cognitive=22  nesting=5

    Summary: 2 functions, avg cyclomatic=11.5, max nesting=5
    ```

---

## Terminal output (readable risk triage)

Top issue rows include all core complexity dimensions in one compact value:

- `CC=<cyclomatic> | Cog=<cognitive> | Nest=<nesting>`

Example:

```text
BoolChain      CC=5 | Cog=3 | Nest=1
NestedIf       CC=4 | Cog=6 | Nest=3
```

Issues are sorted numerically by cyclomatic complexity (then cognitive, then nesting) so the highest-risk functions consistently appear first.

---

## Plot output (fast visual scanning)

The plot output emphasizes fast interpretation for code-review and planning:

- Scatter view includes explicit cyclomatic/cognitive warning guide lines.
- Bubble size maps to nesting depth to preserve one-glance hotspot detection.
- Bar and pie views keep the same threshold semantics as terminal output.

---

## See also

- [Understanding complexity metrics](../explanation/complexity.md) — the mental model, McCabe vs cognitive, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

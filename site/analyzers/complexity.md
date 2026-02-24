# Complexity Analyzer

The complexity analyzer measures three dimensions of code complexity from source code: **cyclomatic complexity**, **cognitive complexity**, and **nesting depth**. It operates on the UAST representation of your source files.

---

## Quick Start

```bash
uast parse main.go | codefang analyze -a complexity
```

Or analyze an entire directory:

```bash
codefang analyze -a complexity ./src/
```

---

## What It Measures

### Cyclomatic Complexity (McCabe, 1976)

Counts the number of linearly independent paths through a function's control flow graph.

For Go code, the analyzer follows the same practical counting model used by `gocyclo`:

- Base score is `1`
- `if`, `for` / `range` add `+1`
- Non-default `case` adds `+1`
- Each `&&` and `||` adds `+1`
- `default` case does **not** add complexity

!!! info "Interpretation"
    - **1-10**: Simple, low risk
    - **11-20**: Moderate complexity
    - **21-50**: High complexity, consider refactoring
    - **51+**: Very high risk, untestable

### Cognitive Complexity (SonarSource, 2017)

Measures how difficult code is for a human to read and understand. Unlike cyclomatic complexity, it penalizes nested structures more heavily and rewards linear sequences of logic.

Key differences from cyclomatic complexity:

- Nesting increments add to the score (deeper nesting = higher penalty)
- `else if` adds structural complexity, but avoids extra nesting penalty vs deeply nested `if`
- Logical operator sequences account for readability, not just path count
- Direct recursion adds a penalty

### Nesting Depth

Tracks the maximum depth of nested control structures within each function. Deep nesting is a strong signal for refactoring.

---

## Configuration Options

The complexity analyzer uses the UAST directly and has no analyzer-specific configuration options. Language support is determined by the UAST parser.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example Output

=== "JSON"

    ```json
    {
      "complexity": {
        "functions": [
          {
            "name": "processFile",
            "file": "main.go",
            "line": 42,
            "cyclomatic": 8,
            "cognitive": 12,
            "nesting_depth": 3
          },
          {
            "name": "validate",
            "file": "main.go",
            "line": 105,
            "cyclomatic": 15,
            "cognitive": 22,
            "nesting_depth": 5
          }
        ],
        "summary": {
          "total_functions": 2,
          "avg_cyclomatic": 11.5,
          "avg_cognitive": 17.0,
          "max_cyclomatic": 15,
          "max_nesting_depth": 5
        }
      }
    }
    ```

=== "Text"

    ```
    Complexity Analysis
      processFile  (main.go:42)   cyclomatic=8   cognitive=12  nesting=3
      validate     (main.go:105)  cyclomatic=15  cognitive=22  nesting=5

    Summary: 2 functions, avg cyclomatic=11.5, max nesting=5
    ```

---

## Validation Against Golden Implementations

The complexity analyzer is continuously validated against battle-tested Go references:

- **Cyclomatic parity target:** `gocyclo` (v0.6.0)
- **Cognitive parity target:** `gocognit` (v1.2.1)

For stabilization, we run a controlled methodology sample through both references and `codefang`, then align discrepancies until parity is reached.

---

## Terminal UX (Readable Risk Triage)

Top issue rows include all core complexity dimensions in one compact value:

- `CC=<cyclomatic> | Cog=<cognitive> | Nest=<nesting>`

Example:

```text
BoolChain      CC=5 | Cog=3 | Nest=1
NestedIf       CC=4 | Cog=6 | Nest=3
```

Issues are sorted numerically by cyclomatic complexity (then cognitive, then nesting) so the highest-risk functions consistently appear first.

---

## Plot UX (Fast Visual Scanning)

The plot output emphasizes fast interpretation for code-review and planning:

- Scatter view includes explicit cyclomatic/cognitive warning guide lines.
- Bubble size maps to nesting depth to preserve one-glance hotspot detection.
- Bar and pie views keep the same threshold semantics as terminal output.

---

## Use Cases

- **Code review gates**: Reject pull requests where any function exceeds a cyclomatic complexity threshold.
- **Refactoring prioritization**: Sort functions by cognitive complexity to find the hardest-to-understand code.
- **Technical debt tracking**: Monitor complexity trends across releases.
- **Test planning**: Functions with high cyclomatic complexity need more test cases for full path coverage.

---

## Limitations

- **Language coverage**: Only languages supported by the UAST parser are analyzed. Unsupported files are silently skipped.
- **Generated code**: The analyzer does not distinguish hand-written code from generated code. Consider excluding generated directories.
- **Macros and metaprogramming**: Complexity within macros or template metaprogramming may not be fully captured, since the UAST represents the source as written, not as expanded.
- **Cognitive complexity model**: The cognitive complexity scoring follows the SonarSource specification. Other tools may use slightly different weightings.

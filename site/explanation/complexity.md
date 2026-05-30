# Understanding complexity metrics

This page explains the mental model behind the complexity analyzer: what
each metric means, why the analyzer measures three dimensions, and how
cyclomatic and cognitive complexity differ. For configuration keys and the
output schema, see the [Complexity reference](../analyzers/complexity.md).

---

## What it measures

The complexity analyzer measures three dimensions of code complexity from
source code: **cyclomatic complexity**, **cognitive complexity**, and
**nesting depth**. It operates on the UAST representation of your source
files.

### Cyclomatic complexity (McCabe, 1976)

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

### Cognitive complexity (SonarSource, 2017)

Measures how difficult code is for a human to read and understand. Unlike cyclomatic complexity, it penalizes nested structures more heavily and rewards linear sequences of logic.

Key differences from cyclomatic complexity:

- Nesting increments add to the score (deeper nesting = higher penalty)
- `else if` adds structural complexity, but avoids extra nesting penalty vs deeply nested `if`
- Logical operator sequences account for readability, not just path count
- Direct recursion adds a penalty

### Nesting depth

Tracks the maximum depth of nested control structures within each function. Deep nesting is a strong signal for refactoring.

---

## Validation against golden implementations

The complexity analyzer is continuously validated against battle-tested Go references:

- **Cyclomatic parity target:** `gocyclo` (v0.6.0)
- **Cognitive parity target:** `gocognit` (v1.2.1)

For stabilization, we run a controlled methodology sample through both references and `codefang`, then align discrepancies until parity is reached.

---

## Use cases

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

---

## See also

- [Complexity reference](../analyzers/complexity.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

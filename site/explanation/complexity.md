# Understanding complexity metrics

This page explains the mental model behind the complexity analyzer: what
each metric means, the exact counting rules, how cyclomatic and cognitive
complexity differ, and how the rules are validated. For the output schema
and the thresholds, see the [Complexity reference](../analyzers/complexity.md).

---

## What it measures

The complexity analyzer measures three dimensions of code complexity from
source code: **cyclomatic complexity**, **cognitive complexity**, and
**nesting depth**, plus the **lines of code** of each function. It operates
on the UAST representation of your source files, with the file's source text
at hand so operators, identifiers and line counts can be recovered where a
language mapping does not carry them.

### Cyclomatic complexity (McCabe, 1976)

Counts the number of linearly independent paths through a function's
control-flow graph. The same rules apply to every language:

- Base score is `1`
- `+1` per `if`, `else if` / `elif`, and conditional (ternary) expression
- `+1` per loop (`for`, `while`, `do`, `loop`, `for … in`, `range`)
- `+1` per non-default `case` / match arm (`default`, `_`, `else` and
  Python's `case _:` add nothing)
- `+1` per `catch` / `except`
- `+1` per logical operator (`&&`, `||`, `and`, `or`)

!!! info "Interpretation"
    - **1-10**: Simple, low risk
    - **11-20**: Moderate complexity
    - **21-50**: High complexity, consider refactoring
    - **51+**: Very high risk, untestable

### Cognitive complexity (SonarSource, 2017)

Measures how difficult code is for a human to read and understand. Unlike
cyclomatic complexity, it penalizes nested structures more heavily and
rewards linear sequences of logic.

- **Structural increments**: `if`, conditional expressions,
  `switch`/`match`, loops and `catch` each add `1` **plus the current
  nesting level**, and raise the nesting level for what they contain.
- **Flat increments**: `else` and `else if` add `1` each and never a nesting
  penalty; an `else if` chain stays at the level of its first `if`.
- **Sequences of logical operators**: `+1` per run of like operators —
  `a && b && c` is one, `a && b || c` is two. A sequence is counted once
  wherever it appears (conditions, returns, assignments, call arguments).
- **Jumps to a label**: `break label`, `continue label` and `goto` add `1`;
  a plain `break`/`continue` adds nothing.
- **Direct recursion**: a call to the function itself adds `1`.
- **Nesting only**: lambdas and anonymous function literals (closures,
  callbacks) add nothing themselves but raise the nesting level for what
  they contain. `try` adds nothing; its `catch` is a structural increment.
- The function's own body is nesting level 0 — including an arrow or
  function-expression wrapper such as `const f = (x) => { … }`.
- A **named nested function** (Python `def` inside `def`, an inner `fn`,
  `const g = function () {}`) is its own unit: it is reported separately and
  excluded from the enclosing function's metrics.

### Nesting depth

The maximum depth of nested control-flow blocks within a function: each
`if`/`else` chain (one level for the whole chain), loop, `switch`/`match`
and `try`/`catch` statement (one level for the whole statement) opens a
level. Conditional expressions and lambdas are not blocks and do not count.
Deep nesting is a strong signal for refactoring.

### Lines of code and density

Lines of code are the non-blank lines of the function's span that carry
something other than comment text (comment-only lines and blank lines are
skipped; a trailing comment does not blank a code line). The lines of a
nested named function belong to that function. `complexity_density` is
cyclomatic complexity divided by lines of code.

---

## Counting rules

### What is a function

A `Function`/`Method` node with a body: a block, an expression body, or an
anonymous function/lambda it wraps (JS declarations). Declarations without a
body — C prototypes and declarators, Go function types and interface
methods, Rust trait signatures, Java abstract and interface methods — are
never reported. Anonymous function literals inside a function belong to
that function (they raise nesting); at the top level they are reported as
`anonymous` only when the mapping exposes them as functions.

Names come from the mapping's `name` property, else from the `Name`-role
identifier (through a C declarator), falling back to the identifier's source
text; a multi-line token is never a name.

### Worked example

```go
func decisions(a, b int, xs []int) int {
    r := 0
    if a > 0 && b > 0 {        // cyclomatic +1 (if) +1 (&&) | cognitive +1 (if) +1 (&& sequence)
        r++
    } else if a < 0 || b < 0 { // cyclomatic +1 (if) +1 (||) | cognitive +1 (else if) +1 (|| sequence)
        r--
    } else {                   //                           | cognitive +1 (else)
        r = 2
    }
    for _, x := range xs {     // cyclomatic +1              | cognitive +1
        r += x
    }
    switch r {                 //                           | cognitive +1
    case 1:                    // cyclomatic +1
        r = 10
    case 2:                    // cyclomatic +1
        r = 20
    default:                   // nothing
        r = 0
    }
    return r
}
```

Cyclomatic `1 + 7 = 8`, cognitive `7`, nesting depth `1` (nothing is nested
inside another block), lines of code `22`.

Nesting changes the cognitive score but not the cyclomatic one:

```go
func nested(a, b, c bool) int {
    if a {                 // cognitive +1 (nesting 0)
        if b {             // cognitive +2 (nesting 1)
            if c {         // cognitive +3 (nesting 2)
                return 1
            }
        }
    }
    return 0
}
```

Cyclomatic `4`, cognitive `6`, nesting depth `3`.

### Operator recovery

A binary operator is read from the node's `operator` property when the
language mapping provides one, else from the node's own token when that is
a bare operator symbol, else from the source text between the two operands.
Python's `and`/`or` are recognised through the same source-text rule even
where the mapping lowers them to a generic node.

---

## Validation

The counting rules are pinned by hand-computed fixtures, not by another
tool's output:

- `crates/cf-complexity/tests/fixtures/` holds Go, Python, JavaScript, Rust,
  Java and C sources (else-if chains, nested `else`, boolean sequences,
  `if` with an init statement, `try`/`catch`, lambdas and closures,
  `switch` with several cases, `default`/`_` arms, recursion, labelled
  jumps, nested functions, declarations without bodies, generics). Every
  function's cyclomatic, cognitive, nesting and line count is asserted
  from the parsed source (`crates/cf-complexity/tests/fixtures.rs`).
- `bins/codefang/tests/complexity_e2e.rs` drives the real binary over the
  same sources and asserts the exact numbers in `json`, `yaml`, `bin` and
  `text`, that `--per-file` entries sum to the aggregate, that every format
  survives 20 000 nesting levels, and that output is deterministic.
- Unit tests pin each rule on hand-built trees (`crates/cf-complexity/src`).

One expectation is recorded but not yet enforced: Java `switch` arms and
the Java ternary are lowered by the present mapping in a shape the analyzer
cannot count (`java_decisions_method` is `#[ignore]`d until the UAST mapping
contract lands); every other Java construct is covered.

---

## Use cases

- **Code review gates**: Reject pull requests where any function exceeds a cyclomatic complexity threshold.
- **Refactoring prioritization**: Sort functions by cognitive complexity to find the hardest-to-understand code.
- **Technical debt tracking**: Monitor complexity trends across releases.
- **Test planning**: Functions with high cyclomatic complexity need more test cases for full path coverage.

---

## Limitations

- **Language coverage**: Only languages supported by the UAST parser are analyzed. Unsupported files are silently skipped.
- **Mapping fidelity**: the analyzer counts what the language mapping exposes. Where a mapping lowers a construct to a generic node (with the present mapping: Java `switch` arms and the Java ternary), that construct is not counted until the mapping is fixed.
- **Generated code**: The analyzer does not distinguish hand-written code from generated code. Consider excluding generated directories (the default path policy already skips vendored and generated paths).
- **Macros and metaprogramming**: Complexity within macros or template metaprogramming may not be fully captured, since the UAST represents the source as written, not as expanded.
- **Cognitive complexity model**: The cognitive complexity scoring follows the SonarSource specification. Other tools may use slightly different weightings (for example, `gocognit` does not count `goto`).

---

## See also

- [Complexity reference](../analyzers/complexity.md) — output schema and thresholds.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

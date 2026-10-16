# Understanding Halstead metrics

This page explains the mental model behind the Halstead analyzer: what
operators and operands are, how codefang finds them in the UAST, how a
codebase is aggregated, and which derived metrics matter most. For the
output schema and the exact thresholds, see the
[Halstead reference](../analyzers/halstead.md).

---

## What it measures

The Halstead model (1977) treats a program as a sequence of **operators**
(symbols and keywords that *do* something: `+`, `:=`, `return`, `if`, a
call's parentheses, a member access) and **operands** (the things they act
on: variables, literals, names). From four counts it derives:

| Metric | Formula | Description |
|---|---|---|
| **n1** | -- | Number of distinct operators |
| **n2** | -- | Number of distinct operands |
| **N1** | -- | Total number of operators |
| **N2** | -- | Total number of operands |
| **Vocabulary** (n) | n1 + n2 | Distinct tokens used |
| **Length** (N) | N1 + N2 | Total tokens in the program |
| **Volume** (V) | N · log2(n) | Information content in bits |
| **Difficulty** (D) | (n1/2) · (N2/n2) | Error proneness |
| **Effort** (E) | D · V | Mental effort to implement |
| **Time** (T) | E / 18 | Estimated implementation time (seconds) |
| **Bugs** (B) | V / 3000 | Estimated delivered bugs |

!!! info "Key insight"
    **Volume** measures the size of the implementation. **Difficulty**
    captures how error-prone it is. Their product, **Effort**, is the best
    single number for comparing overall complexity.

### A worked example

```go
func hal(a int, b int) int {
    c := a + b
    return c * 2
}
```

| | Tokens | Count |
|---|---|---|
| operators | `:=`, `+`, `*`, `return` | n1 = 4, N1 = 4 |
| operands | `a`, `b`, `c`, `c`, `2` | n2 = 4, N2 = 5 |

n = 8, N = 9, V = 9 · log2(8) = **27**, D = (4/2) · (5/4) = **2.5**,
E = **67.5**, T = 3.75 s, B = 0.009. The function's name, its parameters
and its return type are its *signature* — they are declared, not operated
on — so `a` and `b` count once each (their uses in the body), not three
times.

---

## Counting policy

Codefang classifies UAST nodes inside each function body with one table
(the full version is in the [reference](../analyzers/halstead.md#classification-table)):

- **Operands** are leaf `Identifier` and `Literal` nodes, named by their
  text. A quoted string is one operand with its quotes; a qualified name
  such as `fmt.Println` is two operands (`fmt`, `Println`) plus the
  member-access operator `.`.
- **Operators** are, in order of preference: the node's explicit
  `operator` property when the language mapping exposes it; the fixed
  operator of calls (`()`), indexing (`[]`) and slicing (`[:]`); otherwise
  the node's **residue** — its source text with every child blanked out.
  For `c := a + b` the declaration's residue is ` := ` and the binary
  expression's is ` + `; for `return c * 2` it is `return` and its trailing space; for `r++` it
  is `++`. Residue is tokenized into symbol runs (always operators) and
  words (operators only when they are keywords: `return`, `if`, `for`,
  `let`, `int`, `new`, …). Brackets, commas, semicolons and a lone `:` are
  punctuation, not operators; the conditional `? :` counts as one `?:`.
- A leaf identifier with the `Type` role inside a body (`int` in
  `int c = 0;`, `var x int`) is a type-specifier **operator**, following
  the classic convention for declaration keywords.
- **Skipped**: the function's signature (name, parameters, modifiers,
  return type), parameter subtrees anywhere, comments and docstrings.
- **Closures and lambdas** belong to the function that contains them (the
  same rule the complexity analyzer uses); nested *named* functions and
  methods are separate records.

Text is recovered from the file bytes by byte span, so languages whose
mappings leave literals or identifiers without a token (Rust, Java, Go
literals) are counted correctly. When the analyzer runs without the file
(the `history/quality` path), it falls back to the node's own verbatim
token or the enclosing function's token; a node whose text cannot be
recovered is left uncounted — a node is never counted under its type name,
and a statement's text is never an operand.

### Aggregation: union, not average

A file, and the whole codebase, is aggregated as the **union of its
functions' multisets**: an operator used in ten functions is *one*
distinct operator (n1) with its occurrences summed (N1); the derived
measures are then computed from the four union counts exactly as for a
single function. Alongside the union, the report carries per-function
statistics — mean and maximum volume, difficulty and effort, and the
number and share of **high-effort functions** (effort ≥ 10000).

The status message, the health score and the section score are keyed off
that share: a codebase where fewer than 5% of functions are high-effort
is "well-structured", up to 15% "acceptable", up to 30% "consider
refactoring", beyond that "significant refactoring recommended"; the
health score falls linearly from 100 through 90, 70 and 40 at those
boundaries to 0 when every function is high-effort. Note that the *union*
difficulty of a large codebase grows with code size (every operand reuse
raises `N2 / n2`), which is why the health score is based on the
per-function distribution rather than on the union D or E.

---

## Use cases

- **Effort estimation**: compare the Effort of modules or features to rank
  them by the mental work they represent.
- **Bug prediction**: the Bugs metric is a rough upper bound on expected
  defects; functions with high estimated bugs warrant more thorough testing.
- **Code review guidance**: functions with high Difficulty are more
  error-prone and deserve extra scrutiny; `--per-file` shows which files
  concentrate them.
- **Language comparison**: the counts are abstract operator / operand
  counts, so trends compare across languages within one toolchain.

---

## Limitations

- **Cross-tool variance**: language-specific tools (for example, Python-
  or ESTree-only analyzers) differ in tokenization and counting policy.
  Compare trends within one toolchain rather than mixing absolute numbers
  across tools.
- **Mapping coverage**: the counts are as good as the UAST mapping of the
  language. A construct the mapping lowers to a bare wrapper without spans
  is not counted; keyword operators come from a fixed whitelist, so an
  unlisted keyword is dropped rather than mislabelled.
- **Estimation accuracy**: the Bugs and Time formulas are empirical
  approximations from the 1970s. Treat them as relative indicators, not
  precise predictions.
- **Macro expansion**: Halstead metrics count tokens as written, not as
  expanded. Heavy use of macros or code generation can skew results.
- **Comments excluded**: comments and whitespace are excluded from
  Halstead counts (by design).

---

## See also

- [Halstead reference](../analyzers/halstead.md) — classification table, thresholds and output schema.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

# Halstead Analyzer

The Halstead analyzer computes **Halstead complexity metrics** (1977) based on operator and operand counts extracted from the UAST. These metrics provide an objective, quantitative assessment of program size and complexity.

---

## Quick Start

```bash
# Analyze a repository or directory
codefang run -a static/halstead .

# Human-readable terminal output
codefang run -a static/halstead --format text .

# Interactive plots
codefang run -a static/halstead --format plot . > halstead.html
```

---

## What It Measures

The Halstead model treats a program as a sequence of **operators** (keywords, symbols, function calls) and **operands** (variables, literals, constants). From these counts, it derives:

| Metric | Formula | Description |
|---|---|---|
| **n1** | -- | Number of distinct operators |
| **n2** | -- | Number of distinct operands |
| **N1** | -- | Total number of operators |
| **N2** | -- | Total number of operands |
| **Vocabulary** (n) | n1 + n2 | Distinct tokens used |
| **Length** (N) | N1 + N2 | Total tokens in the program |
| **Volume** (V) | N * log2(n) | Information content in bits |
| **Difficulty** (D) | (n1/2) * (N2/n2) | Error proneness |
| **Effort** (E) | D * V | Mental effort to implement |
| **Time** (T) | E / 18 | Estimated implementation time (seconds) |
| **Bugs** (B) | V / 3000 | Estimated delivered bugs |

!!! info "Key insight"
    **Volume** measures the size of the implementation. **Difficulty** captures how error-prone it is. Their product, **Effort**, is the best single number for comparing overall complexity.

### Counting policy

Codefang computes Halstead metrics from UAST with a lexical-first policy:

- Counts lexical operands (`Identifier`, `Literal`, `Field`) and operator nodes/roles.
- Excludes structural wrappers and declaration-only artifacts from operand counts.
- Uses explicit operator properties when available, then token extraction fallback.

This improves stability across languages and avoids pseudo-operands like structural `Parameter` nodes.

---

## Configuration Options

The Halstead analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Output Interpretation

### Terminal (text/summary)

The section now surfaces foundational counts before derived metrics:

- `Distinct Operators (n1)`
- `Distinct Operands (n2)`
- `Total Operators (N1)`
- `Total Operands (N2)`
- `Vocabulary`, `Volume`, `Difficulty`, `Effort`, `Est. Bugs`

Top issues include compact multi-signal context:

- `effort=<...> | vol=<...> | bugs=<...>`

Severity is determined from both effort and bug estimate.

### Plot

The plot report includes:

1. **Top Functions by Effort** (Top 12, highest first)
2. **Volume vs Difficulty** risk map:
   - X: volume
   - Y: difficulty
   - bubble size: estimated bugs
   - color: low/medium/high risk bucket
3. **Volume Distribution** by bucket (`Low`, `Medium`, `High`, `Very High`)

## Use Cases

- **Effort estimation**: Use the Effort metric to compare the relative complexity of different modules or features.
- **Bug prediction**: The Bugs metric provides a rough upper bound on expected defects. Modules with high estimated bugs warrant more thorough testing.
- **Code review guidance**: Functions with high Difficulty scores are more error-prone and deserve extra scrutiny.
- **Language comparison**: Halstead metrics allow cross-language comparisons since they are based on abstract operator/operand counts.

---

## Limitations

- **Cross-tool variance**: Language-specific tools (for example, Python- or ESTree-only analyzers) can differ in tokenization and counting policy. Compare trends within one toolchain rather than mixing absolute numbers across tools.
- **Operator classification**: UAST-based classification targets cross-language consistency, not byte-for-byte parity with each language parser.
- **Estimation accuracy**: The Bugs and Time formulas are empirical approximations from the 1970s. Treat them as relative indicators, not precise predictions.
- **Macro expansion**: Halstead metrics count tokens as written, not as expanded. Heavy use of macros or code generation can skew results.
- **Comments excluded**: Comments and whitespace are excluded from Halstead counts (by design).

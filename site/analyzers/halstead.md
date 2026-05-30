# Halstead analyzer reference

The Halstead analyzer computes **Halstead complexity metrics** (1977) based on
operator and operand counts extracted from the UAST.

For the conceptual model — what operators and operands are and which derived
metrics matter most — see
[Understanding Halstead metrics](../explanation/halstead.md). To run it, see
the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The Halstead analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Output formats

### Terminal (text/summary)

The section surfaces foundational counts before derived metrics:

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

---

## See also

- [Understanding Halstead metrics](../explanation/halstead.md) — the mental model, formulas, counting policy, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

# Halstead Analyzer

The Halstead analyzer computes **Halstead complexity metrics** (1977) based on operator and operand counts extracted from the UAST. These metrics provide an objective, quantitative assessment of program size and complexity.

---

## Quick Start

```bash
uast parse main.go | codefang analyze -a halstead
```

Or analyze an entire directory:

```bash
codefang analyze -a halstead ./src/
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

---

## Configuration Options

The Halstead analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example Output

=== "JSON"

    ```json
    {
      "halstead": {
        "functions": [
          {
            "name": "processFile",
            "file": "main.go",
            "line": 42,
            "n1": 15,
            "n2": 22,
            "N1": 48,
            "N2": 53,
            "vocabulary": 37,
            "length": 101,
            "volume": 526.3,
            "difficulty": 18.1,
            "effort": 9521.0,
            "time": 529.0,
            "bugs": 0.18
          }
        ],
        "summary": {
          "total_functions": 1,
          "total_volume": 526.3,
          "avg_difficulty": 18.1,
          "total_estimated_bugs": 0.18
        }
      }
    }
    ```

=== "Text"

    ```
    Halstead Complexity Metrics
      processFile (main.go:42)
        vocabulary=37  length=101  volume=526.3
        difficulty=18.1  effort=9521  time=529s  bugs=0.18

    Summary: 1 function, total volume=526.3, est. bugs=0.18
    ```

---

## Use Cases

- **Effort estimation**: Use the Effort metric to compare the relative complexity of different modules or features.
- **Bug prediction**: The Bugs metric provides a rough upper bound on expected defects. Modules with high estimated bugs warrant more thorough testing.
- **Code review guidance**: Functions with high Difficulty scores are more error-prone and deserve extra scrutiny.
- **Language comparison**: Halstead metrics allow cross-language comparisons since they are based on abstract operator/operand counts.

---

## Limitations

- **Operator classification**: The UAST-based operator/operand classification may differ slightly from the original Halstead definitions, which were designed for Fortran. Results are internally consistent but may not match tools that use language-specific tokenizers.
- **Estimation accuracy**: The Bugs and Time formulas are empirical approximations from the 1970s. Treat them as relative indicators, not precise predictions.
- **Macro expansion**: Halstead metrics count tokens as written, not as expanded. Heavy use of macros or code generation can skew results.
- **Comments excluded**: Comments and whitespace are excluded from Halstead counts (by design).

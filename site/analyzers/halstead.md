# Halstead analyzer reference

The Halstead analyzer computes **Halstead complexity metrics** (1977) from
the operators and operands of every function in the UAST.

For the conceptual model — what counts as an operator or operand, how the
codebase aggregate is defined and what the thresholds mean — see
[Understanding Halstead metrics](../explanation/halstead.md). To run it, see
the [Quick start](../getting-started/quickstart.md).

```sh
codefang run -a static/halstead --format json ./src
codefang run -a static/halstead --format json --per-file ./src
codefang run -a static/halstead --format yaml ./src
codefang run -a static/halstead --format plot --output ./out ./src
```

---

## Configuration options

The Halstead analyzer uses the UAST directly and has no analyzer-specific
configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

The run-level flags apply: `--per-file` (json only — yaml and bin always
report the codebase aggregate), `--include-vendored` / `--include-generated`,
and `--languages`.

---

## Definitions

| Symbol | Meaning | Formula |
|---|---|---|
| `n1` | distinct operators | — |
| `n2` | distinct operands | — |
| `N1` | total operators | — |
| `N2` | total operands | — |
| Vocabulary `n` | | `n1 + n2` |
| Length `N` | | `N1 + N2` |
| Estimated length | | `n1·log2(n1) + n2·log2(n2)` |
| Volume `V` | | `N · log2(n)` |
| Difficulty `D` | | `(n1 / 2) · (N2 / n2)` |
| Effort `E` | | `D · V` |
| Time `T` (seconds) | | `E / 18` |
| Bugs `B` | | `V / 3000` |

### Classification table

| UAST node (inside a function body) | Counted as | Name |
|---|---|---|
| leaf `Identifier`, leaf `Literal` | operand | its text (`token`, else the source span, else `props.name` / `props.value`) |
| leaf `Identifier` with the `Type` role | operator (type specifier) | its text |
| any node with `props["operator"]` | operator | that property, and nothing else from the node |
| `Call` / `Index` / `Slice` | operator | fixed `()` / `[]` / `[:]` |
| every other node | operator(s) | the node's *residue*: its source span with its children blanked (` := `, ` + `, `return `, `.`, `++`), split into symbol runs and whitelisted keywords |
| `Parameter` (type or role), `Comment`, `DocString` | skipped | — |
| the function's signature (every child before its body) | skipped | — |

A quoted literal (`"x"`) is one operand including its quotes; a composite
literal or qualified name is classified by its parts, so `fmt.Println`
yields the operands `fmt`, `Println` and the operator `.`. Closures and
lambdas belong to the function that contains them; nested named functions
and methods are their own records. A node type name is never a token and a
statement's text is never an operand. The keyword whitelist and the
canonical fallbacks are in `crates/cf-halstead/src/classify.rs`.

### Aggregate

The codebase aggregate — and each `--per-file` entry — is the **union** of
the operator and operand multisets of its functions: `n1` / `n2` are
distinct across every function, `N1` / `N2` are the summed occurrences, and
V / D / E / T / B derive from those four numbers. Per-function statistics
(mean and max volume, difficulty and effort; the high-effort count and
share) sit alongside.

### Thresholds

All bands are lower-inclusive (a value at a bound is in the higher band).

| Per function | Low / Simple | Medium / Moderate | High / Complex | Very High |
|---|---|---|---|---|
| Volume `V` (distribution, `complexity_level`) | `V < 100` | `100 ≤ V < 1000` | `1000 ≤ V < 5000` | `V ≥ 5000` |
| Difficulty `D` (`difficulty_assessment`) | `D < 5` | `5 ≤ D < 15` | `D ≥ 15` | — |
| Effort `E` (`effort_assessment`) | `E < 1000` | `1000 ≤ E < 10000` | `E ≥ 10000` | — |

* **High-effort function**: `E ≥ 10000` (listed under
  `high_effort_functions`; `risk_level` is `HIGH` from `E ≥ 50000`, else
  `MEDIUM`).
* **Issue severity**: `poor` when `E ≥ 50000` or `B ≥ 1.0`; `fair` when
  `E ≥ 10000` or `B ≥ 0.3`; else `good`.
* **Status message and health score** come from the share `h` of
  high-effort functions:

| `h` | `status` / `message` | `health_score` |
|---|---|---|
| no functions | `No functions found` | 100 |
| `h < 5%` | `Low Halstead complexity - well-structured code` | 100 → 90 |
| `5% ≤ h < 15%` | `Moderate Halstead complexity - acceptable` | 90 → 70 |
| `15% ≤ h < 30%` | `High Halstead complexity - consider refactoring` | 70 → 40 |
| `h ≥ 30%` | `Very high Halstead complexity - significant refactoring recommended` | 40 → 0 |

The health score interpolates linearly between the anchors
`(0, 100)`, `(5%, 90)`, `(15%, 70)`, `(30%, 40)`, `(100%, 0)`. The section
`score` is `health_score / 100` and `score_label` is `round(score · 10)/10`.

Function lists (`issues`, `function_halstead`, `high_effort_functions`) are
ordered by effort descending, then volume descending, then name, then file.

---

## Output formats

The examples below are the real output for a two-file fixture: `hal.go`
(`c := a + b; return c * 2`) and `calls.go` (three `fmt.Println(...)`
calls).

### JSON (`--format json`)

```json
{
  "overall_score_label": "10/10",
  "sections": [
    {
      "title": "HALSTEAD",
      "score_label": "10/10",
      "status": "Low Halstead complexity - well-structured code",
      "metrics": [
        { "label": "Total Functions", "value": "2" },
        { "label": "Distinct Operators (n1)", "value": "6" },
        { "label": "Distinct Operands (n2)", "value": "8" },
        { "label": "Total Operators (N1)", "value": "10" },
        { "label": "Total Operands (N2)", "value": "14" },
        { "label": "Vocabulary", "value": "14" },
        { "label": "Length", "value": "24" },
        { "label": "Volume", "value": "91.4" },
        { "label": "Difficulty", "value": "5.2" },
        { "label": "Effort", "value": "479.7" },
        { "label": "Est. Bugs", "value": "0.0" },
        { "label": "Mean Effort", "value": "77.4" },
        { "label": "Max Effort", "value": "87.2" },
        { "label": "High-Effort Functions", "value": "0" }
      ],
      "distribution": [
        { "label": "Low (<100)", "percent": 1, "count": 2 },
        { "label": "Medium (100-1000)", "percent": 0, "count": 0 },
        { "label": "High (1000-5000)", "percent": 0, "count": 0 },
        { "label": "Very High (>=5000)", "percent": 0, "count": 0 }
      ],
      "issues": [
        { "name": "calls", "location": "calls.go", "value": "effort=87.2 | vol=38.8 | bugs=0.0", "severity": "good" },
        { "name": "hal", "location": "hal.go", "value": "effort=67.5 | vol=27.0 | bugs=0.0", "severity": "good" }
      ],
      "score": 1
    }
  ],
  "overall_score": 1
}
```

With `--per-file` the section gains a `files` array with one entry per
analyzed file (function-free files included), each carrying the same
`metrics` list computed from that file's own functions, its own
`distribution` (absent when the file has no functions), its `issues`, and
a `score` from its own high-effort share:

```json
"files": [
  {
    "file_path": "calls.go",
    "score_label": "10/10",
    "status": "Low Halstead complexity - well-structured code",
    "metrics": [
      { "label": "Total Functions", "value": "1" },
      { "label": "Distinct Operators (n1)", "value": "2" },
      { "label": "Distinct Operands (n2)", "value": "4" },
      { "label": "Total Operators (N1)", "value": "6" },
      { "label": "Total Operands (N2)", "value": "9" },
      ...
    ],
    "distribution": [ ... ],
    "issues": [ ... ],
    "score": 1
  },
  ...
]
```

A folder with no functions (or no analyzable files) produces the same shape
with every count at 0, `"status": "No functions found"`, no `distribution`,
and empty `issues` / `files`; the exit code is 0.

### YAML / bin (`--format yaml`, `--format bin`)

Both formats serialize the same value (bin wraps it in the CFB1 envelope):

```yaml
function_halstead:
    - name: calls
      source_file: calls.go
      language: go
      directory: .
      volume: 38.77443751081734
      difficulty: 2.25
      effort: 87.24248439933902
      time_to_program: 4.846804688852168
      delivered_bugs: 0.012924812503605781
      complexity_level: Low
    - name: hal
      source_file: hal.go
      language: go
      directory: .
      volume: 27
      difficulty: 2.5
      effort: 67.5
      time_to_program: 3.75
      delivered_bugs: 0.009
      complexity_level: Low
distribution:
    low: 2
    medium: 0
    high: 0
    very_high: 0
high_effort_functions: []
aggregate:
    total_functions: 2
    volume: 91.3765181293825
    difficulty: 5.25
    effort: 479.72672017925817
    time_to_program: 26.65148445440323
    delivered_bugs: 0.030458839376460836
    distinct_operators: 6
    distinct_operands: 8
    total_operators: 10
    total_operands: 14
    vocabulary: 14
    length: 24
    estimated_length: 39.50977500432694
    mean_volume: 32.88721875540867
    max_volume: 38.77443751081734
    mean_difficulty: 2.375
    max_difficulty: 2.5
    mean_effort: 77.37124219966951
    max_effort: 87.24248439933902
    high_effort_functions: 0
    high_effort_share: 0
    health_score: 100
    message: Low Halstead complexity - well-structured code
```

`high_effort_functions` entries carry `name`, `source_file`, `volume`,
`effort`, `time_to_program`, `delivered_bugs` and `risk_level`.

### Terminal (`--format text`, `--format compact`)

The section shows the status line and the same key-metrics list as JSON
(union counts, derived measures, mean / max effort, high-effort count).

### Plot (`--format plot --output <dir>`)

`<dir>/report.json` carries the raw report under `halstead`: every function
in walk order with its counts, derived measures, assessments and the full
`operators` / `operands` maps, plus the union aggregate, per-function
statistics and `health_score`:

```json
{
  "name": "hal",
  "_source_file": "hal.go", "_language": "go", "_directory": ".",
  "distinct_operators": 4, "distinct_operands": 4,
  "total_operators": 4, "total_operands": 5,
  "vocabulary": 8, "length": 9, "estimated_length": 16,
  "volume": 27, "difficulty": 2.5, "effort": 67.5,
  "time_to_program": 3.75, "delivered_bugs": 0.009,
  "volume_assessment": "🟢 Low",
  "difficulty_assessment": "🟢 Simple",
  "effort_assessment": "🟢 Low",
  "operators": { "*": 1, "+": 1, ":=": 1, "return": 1 },
  "operands": { "2": 1, "a": 1, "b": 1, "c": 2 }
}
```

The HTML page includes **Top Functions by Effort**, the **Volume vs
Difficulty** risk map (bubble size: estimated bugs) and the **Volume
Distribution** by bucket.

---

## See also

- [Understanding Halstead metrics](../explanation/halstead.md) — the mental model, counting policy, aggregation and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

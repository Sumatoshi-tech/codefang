# Complexity analyzer reference

The complexity analyzer measures three dimensions of code complexity from
source code: **cyclomatic complexity**, **cognitive complexity**, and
**nesting depth**, plus the **lines of code** of every function. It operates
on the UAST representation of your source files and reads the file source
alongside it (operators, identifiers and line counts are recovered from the
source where a language mapping does not carry them).

For the conceptual model — what each metric means, the exact counting rules,
and how the rules are validated — see
[Understanding complexity metrics](../explanation/complexity.md). To run it,
see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The complexity analyzer uses the UAST directly and has no analyzer-specific configuration options. Language support is determined by the UAST parser.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

Run-level flags that affect it: `--languages`, `--include-vendored`,
`--include-generated` (which files are walked) and `--per-file` (JSON only,
see below).

---

## Output formats

| Format | Shape | Notes |
|---|---|---|
| `json` | Sections report: one scored `COMPLEXITY` section with `metrics`, `distribution`, `issues` (every function, ranked) | `--per-file` adds a `files` array with one entry per analyzed file (function-free files included) |
| `yaml`, `bin` | Computed metrics: `function_complexity`, `distribution`, `high_risk_functions`, `aggregate` | `bin` wraps the compact JSON of the same value in the `CFB1` envelope; `--per-file` is not applied to these formats |
| `text`, `compact` | Terminal rendering of the `json` section | `text` prints the key metrics, the distribution and the top five functions ("Top Issues"; `--verbose` prints all) |
| `plot` | HTML pages built from the raw aggregated report | `report.json` carries every function with its assessments |

Functions are ordered the same way everywhere: cyclomatic descending, then
cognitive descending, then nesting descending, then name, file and start
line ascending. Ties are therefore deterministic across runs and formats.

---

## Example output

The examples below are the real output for a Go file with ten functions
(`crates/cf-complexity/tests/fixtures/cc.go`, the file the test suite
measures).

=== "JSON"

    ```json
    {
      "overall_score_label": "6/10",
      "sections": [
        {
          "title": "COMPLEXITY",
          "score_label": "6/10",
          "status": "Fair complexity - some functions could be simplified",
          "metrics": [
            { "label": "Total Functions", "value": "10" },
            { "label": "Avg Complexity", "value": "3.6" },
            { "label": "Max Complexity", "value": "8" },
            { "label": "Total Complexity", "value": "36" },
            { "label": "Cognitive Total", "value": "29" },
            { "label": "Max Nesting", "value": "3" },
            { "label": "Decision Points", "value": "26" }
          ],
          "distribution": [
            { "label": "Simple (1-5)", "percent": 0.9, "count": 9 },
            { "label": "Moderate (6-10)", "percent": 0.1, "count": 1 },
            { "label": "Complex (11-20)", "percent": 0, "count": 0 },
            { "label": "Very Complex (>20)", "percent": 0, "count": 0 }
          ],
          "issues": [
            {
              "name": "decisions",
              "location": "cc.go",
              "value": "CC=8 | Cog=7 | Nest=1",
              "severity": "fair"
            },
            {
              "name": "manyCases",
              "location": "cc.go",
              "value": "CC=5 | Cog=1 | Nest=1",
              "severity": "good"
            }
          ],
          "score": 0.6
        }
      ],
      "overall_score": 0.6
    }
    ```

    (`issues` continues with the remaining eight functions.) `Cognitive
    Total` is the sum over all functions and `Max Nesting` the maximum, never
    a per-file average. With `--per-file`, a `files` array sits between
    `issues` and `score`; each entry carries the file's own `metrics`,
    `distribution` (omitted for function-free files), `issues`, `status`
    and `score`, and the per-file metrics sum (or max) to the section's.

=== "YAML"

    ```yaml
    function_complexity:
        - name: decisions
          source_file: cc.go
          language: go
          directory: .
          cyclomatic_complexity: 8
          cognitive_complexity: 7
          nesting_depth: 1
          lines_of_code: 22
          complexity_density: 0.36363636363636365
          risk_level: MEDIUM
        - name: manyCases
          source_file: cc.go
          language: go
          directory: .
          cyclomatic_complexity: 5
          cognitive_complexity: 1
          nesting_depth: 1
          lines_of_code: 14
          complexity_density: 0.35714285714285715
          risk_level: MEDIUM
    distribution:
        moderate: 1
        simple: 9
    high_risk_functions: []
    aggregate:
        total_functions: 10
        average_complexity: 3.6
        max_complexity: 8
        total_complexity: 36
        cognitive_complexity: 29
        nesting_depth: 3
        decision_points: 26
        health_score: 75.5
        message: Fair complexity - some functions could be simplified
    ```

    Each function record includes `source_file`, `language`, and `directory`
    for file-level joins and DWH aggregation. `--format bin` carries exactly
    this value as compact JSON inside the `CFB1` envelope.

=== "Text"

    ```
    ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
    ┃ COMPLEXITY                                                       Score: 6/10 ┃
    ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

      Summary: Fair complexity - some functions could be simplified

      Key Metrics
      ────────────────────────────────────────────────────────────────────────────
      Total Functions     10          Avg Complexity      3.6
      Max Complexity      8           Total Complexity    36
      Cognitive Total     29          Max Nesting         3
      Decision Points     26

      Distribution
      ────────────────────────────────────────────────────────────────────────────
      Simple (1-5)       ████████████████████████████████████░░░░  90%  (9)
      Moderate (6-10)    ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  10%  (1)
      Complex (11-20)    ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   0%  (0)
      Very Complex (>20) ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   0%  (0)

      Top Issues
      ────────────────────────────────────────────────────────────────────────────
      decisions                 cc.go                               CC=8 | Cog=7 | Nest=1
      manyCases                 cc.go                               CC=5 | Cog=1 | Nest=1
      nested                    cc.go                               CC=4 | Cog=6 | Nest=3
      loopCond                  cc.go                               CC=4 | Cog=3 | Nest=1
      boolSeq                   cc.go                               CC=4 | Cog=2 | Nest=0
    ```

---

## Field reference

### Per function (`function_complexity`, `issues`, `report.json: functions`)

| Field | Meaning |
|---|---|
| `name` | The function's identifier (`anonymous` when none can be recovered); never source text |
| `source_file`, `language`, `directory` | File relative to the analyzed root, detected language, directory of the file (`.` at the root) |
| `cyclomatic_complexity` | `1` + decision points (see the [counting rules](../explanation/complexity.md#counting-rules)) |
| `cognitive_complexity` | SonarSource cognitive complexity |
| `nesting_depth` | Maximum depth of nested control-flow blocks |
| `lines_of_code` | Non-blank, non-comment-only lines of the function's span; lines of a nested named function belong to that function |
| `complexity_density` | `cyclomatic_complexity / lines_of_code` (`0` without lines) |
| `risk_level` | `LOW` / `MEDIUM` / `HIGH` / `CRITICAL`, see [Thresholds](#thresholds) |
| `complexity_assessment`, `cognitive_assessment`, `nesting_assessment` | Per-function labels in the raw report (`report.json`), see [Thresholds](#thresholds) |

### Aggregate (`aggregate`, the JSON `metrics`)

| Field | JSON metric | Meaning |
|---|---|---|
| `total_functions` | Total Functions | Number of functions |
| `average_complexity` | Avg Complexity | `total_complexity / total_functions` (function-weighted; `0` without functions) |
| `max_complexity` | Max Complexity | Highest per-function cyclomatic complexity |
| `total_complexity` | Total Complexity | Sum of cyclomatic complexity |
| `cognitive_complexity` | Cognitive Total | Sum of cognitive complexity |
| `nesting_depth` | Max Nesting | Highest per-function nesting depth |
| `decision_points` | Decision Points | Sum of `cyclomatic_complexity - 1` |
| `health_score` | -- | 0..100 from the average, see [Thresholds](#thresholds) |
| `message` | `status` | Band message from the average |

`distribution` (yaml/bin) counts functions per cyclomatic bucket and omits
empty buckets; `high_risk_functions` lists every function crossing at least
one high threshold, ordered by risk level then the function order, with its
`issues` (`High cyclomatic complexity`, `High cognitive complexity`, `Deep
nesting`).

---

## Thresholds

Every threshold is defined once (`cf_complexity::thresholds`) and used by
every format.

**Risk level** (`risk_level`): each dimension scores 2 at or above its high
threshold, 1 at or above its moderate threshold, else 0; the sum maps to a
level.

| Dimension | Moderate (+1) | High (+2) |
|---|---|---|
| Cyclomatic | ≥ 5 | ≥ 10 |
| Cognitive | ≥ 7 | ≥ 15 |
| Nesting | ≥ 3 | ≥ 5 |

| Score | Level |
|---|---|
| 0 | `LOW` |
| 1–2 | `MEDIUM` |
| 3–4 | `HIGH` |
| 5–6 | `CRITICAL` |

Worked examples: cyclomatic 8, cognitive 12, nesting 3 scores 1 + 1 + 1 =
3 → `HIGH`; cyclomatic 15, cognitive 22, nesting 5 scores 2 + 2 + 2 = 6 →
`CRITICAL`. The high thresholds are also what `high_risk_functions`
reports as issues.

**Cyclomatic buckets**: yaml/bin `distribution` uses `simple` (≤ 5),
`moderate` (6–10), `complex` (> 10); the terminal/JSON distribution uses
`Simple (1-5)`, `Moderate (6-10)`, `Complex (11-20)`, `Very Complex (>20)`.
Issue `severity` is `good` (≤ 5), `fair` (6–10), `poor` (≥ 11).

**Per-function assessments** (raw report): cyclomatic ≤ 1 `🟢 Simple`, ≤ 5
`🟡 Moderate`, else `🔴 Complex`; cognitive ≤ 5 `🟢 Low`, ≤ 10 `🟡 Medium`,
else `🔴 High`; nesting ≤ 3 `🟢 Shallow`, ≤ 5 `🟡 Moderate`, else `🔴 Deep`.

**Average complexity bands** (message, section score, health score):

| Average cyclomatic | Message | Score | Health |
|---|---|---|---|
| ≤ 1 | Excellent complexity - functions are simple and maintainable | 10/10 | 100 |
| ≤ 3 | Good complexity - functions have reasonable complexity | 8/10 | 80 + (3 − avg) × 10 |
| ≤ 5 | Fair complexity - some functions could be simplified | 6/10 | 50 + (7 − avg) × 7.5 |
| ≤ 7 | Fair complexity - some functions could be simplified | 4/10 | 50 + (7 − avg) × 7.5 |
| ≤ 10 | High complexity - functions are complex and should be refactored | 2/10 | max(0, 50 − (avg − 7) × 5) |
| > 10 | High complexity - functions are complex and should be refactored | 1/10 | max(0, 50 − (avg − 7) × 5) |

---

## Terminal output (readable risk triage)

Top issue rows include all core complexity dimensions in one compact value:

- `CC=<cyclomatic> | Cog=<cognitive> | Nest=<nesting>`

Example:

```text
decisions                 cc.go                               CC=8 | Cog=7 | Nest=1
nested                    cc.go                               CC=4 | Cog=6 | Nest=3
```

Issues are sorted by cyclomatic complexity (then cognitive, then nesting,
then name and file) so the highest-risk functions consistently appear first.
`text` prints the top five; `--verbose` prints every function.

---

## Plot output (fast visual scanning)

The plot output emphasizes fast interpretation for code-review and planning:

- Scatter view includes explicit cyclomatic/cognitive warning guide lines.
- Bubble size maps to nesting depth to preserve one-glance hotspot detection.
- Bar and pie views keep the same threshold semantics as terminal output.

---

## Troubleshooting

- **A function is missing**: declarations without a body (C prototypes,
  Go interface methods and function types, Rust trait signatures, Java
  abstract/interface methods) are never reported. Anonymous function
  literals outside any function (Go `var f = func() {}`) are not reported
  either; inside a function they count toward that function.
- **Two functions with the same name**: the rows are distinct by
  `source_file` (and start line in the ordering); nothing is merged.
- **Deeply nested sources**: every format handles tens of thousands of
  nesting levels (the walk grows its stack on demand).
- **`--per-file` with `yaml`/`bin`**: the flag only enriches the `json`
  sections report; the other formats are unchanged.

---

## See also

- [Understanding complexity metrics](../explanation/complexity.md) — the mental model, the counting rules, and how they are validated.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

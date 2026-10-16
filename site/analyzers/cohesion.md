# Cohesion analyzer reference

The cohesion analyzer computes **LCOM-HS (Henderson-Sellers)** per file and a
**variable sharing ratio** per function to identify files and modules with
low internal cohesion.

For the conceptual model — what counts as a variable, what LCOM-HS measures
and how to read the scores — see
[Understanding cohesion metrics](../explanation/cohesion.md). To run it, see
the [Quick start](../getting-started/quickstart.md).

```bash
codefang run -a static/cohesion --format text .
codefang run -a static/cohesion --format json --per-file .
```

---

## Configuration options

The cohesion analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

The run-level walk flags apply (`--languages`, `--include-generated`,
`--include-vendored`, `--per-file`).

---

## Metrics and thresholds

| Metric | Meaning |
|---|---|
| Total Functions | Every function found, same-named ones included; methods are reported as `Type.name` |
| LCOM Score | Function-weighted mean of the per-file LCOM-HS values (0 = cohesive, 1 = not) |
| Cohesion Score | `1 - LCOM Score`; the section score (`6/10` = `0.60`) |
| Avg Cohesion | Mean per-function cohesion (shared variables / distinct variables) |

Metrics render with two decimals. Per-function cohesion is bucketed as:

| Bucket | Range | Issue severity |
|---|---|---|
| Excellent | `>= 0.6` | not listed |
| Good | `[0.4, 0.6)` | `good` |
| Fair | `[0.3, 0.4)` | `fair` |
| Poor | `< 0.3` | `poor` |

The distribution always sums to Total Functions. The issues list holds every
function below Excellent, ordered by cohesion ascending, then name; `text`
shows the first five (`-v` for all).

The summary message follows the Cohesion Score: Excellent `>= 0.7`, Good
`>= 0.4`, Fair `>= 0.3`, otherwise Poor.

### No data

Files without functions (struct-only files, empty files, Markdown) are
excluded from every number. When no analyzed file has a function — including
an existing empty directory — the section reports `No cohesion data
available`, `Total Functions 0`, score `0` (`0/10`), no distribution and an
empty issues list, and the command exits 0. Unreadable or unparseable files
are skipped and counted in the `progress: skipped N …` line on stderr.

---

## Example output

The fixture is two Go files — `share.go` with `f` using `{x, y}`, `g` using
`{x}` and `h` using `{x, y, z}` (LCOM 1/3), and `disjoint.go` with `f2` using
`{p}` and `g2` using `{q}` (LCOM 0.5) — plus a struct-only `types.go`.

=== "Text"

    ```
    ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
    ┃ COHESION                                                         Score: 6/10 ┃
    ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

      Summary: Good overall cohesion with room for improvement

      Key Metrics
      ────────────────────────────────────────────────────────────────────────────
      Total Functions     5           LCOM Score          0.40
      Cohesion Score      0.60        Avg Cohesion        0.53

      Distribution
      ────────────────────────────────────────────────────────────────────────────
      Excellent (>=0.6)  ████████████████████████░░░░░░░░░░░░░░░░  60%  (3)
      Good (0.4-0.6)     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   0%  (0)
      Fair (0.3-0.4)     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   0%  (0)
      Poor (<0.3)        ████████████████░░░░░░░░░░░░░░░░░░░░░░░░  40%  (2)

      Top Issues
      ────────────────────────────────────────────────────────────────────────────
      f2                        disjoint.go                         0.00
      g2                        disjoint.go                         0.00
    ```

    `--format compact` renders the one-line form:

    ```
    COHESION     [██████░░░░] 6/10  Good overall cohesion with room for improvement
    ```

=== "JSON"

    `--format json` emits the section report (`text` and `compact` render the
    same section). With `--per-file`, a `files` array sits between `issues`
    and `score`: one entry per analyzed file with that file's own metrics,
    distribution and issues; a function-free file carries
    `"status": "No functions found"`.

    ```json
    {
      "overall_score_label": "6/10",
      "sections": [
        {
          "title": "COHESION",
          "score_label": "6/10",
          "status": "Good overall cohesion with room for improvement",
          "metrics": [
            { "label": "Total Functions", "value": "5" },
            { "label": "LCOM Score", "value": "0.40" },
            { "label": "Cohesion Score", "value": "0.60" },
            { "label": "Avg Cohesion", "value": "0.53" }
          ],
          "distribution": [
            { "label": "Excellent (>=0.6)", "percent": 0.6, "count": 3 },
            { "label": "Good (0.4-0.6)", "percent": 0, "count": 0 },
            { "label": "Fair (0.3-0.4)", "percent": 0, "count": 0 },
            { "label": "Poor (<0.3)", "percent": 0.4, "count": 2 }
          ],
          "issues": [
            { "name": "f2", "location": "disjoint.go", "value": "0.00", "severity": "poor" },
            { "name": "g2", "location": "disjoint.go", "value": "0.00", "severity": "poor" }
          ],
          "score": 0.6
        }
      ],
      "overall_score": 0.6
    }
    ```

=== "YAML / binary"

    `--format yaml` and `--format bin` (a `CFB1` envelope over the compact
    JSON of the same value) emit the computed metrics: every function with
    its quality level (cohesion ascending), the bucket counts (only buckets
    that occur), the functions below Good with a risk level (`HIGH` below
    Fair, `MEDIUM` otherwise), and the aggregate.

    ```yaml
    function_cohesion:
        - name: f2
          source_file: disjoint.go
          cohesion: 0
          quality_level: Poor
        - name: g2
          source_file: disjoint.go
          cohesion: 0
          quality_level: Poor
        - name: h
          source_file: share.go
          cohesion: 0.6666666666666666
          quality_level: Excellent
        - name: f
          source_file: share.go
          cohesion: 1
          quality_level: Excellent
        - name: g
          source_file: share.go
          cohesion: 1
          quality_level: Excellent
    distribution:
        excellent: 3
        poor: 2
    low_cohesion_functions:
        - name: f2
          source_file: disjoint.go
          cohesion: 0
          risk_level: HIGH
          recommendation: Consider splitting into multiple focused functions
        - name: g2
          source_file: disjoint.go
          cohesion: 0
          risk_level: HIGH
          recommendation: Consider splitting into multiple focused functions
    aggregate:
        total_functions: 5
        lcom: 0.4
        lcom_variant: LCOM-HS (Henderson-Sellers)
        cohesion_score: 0.6
        function_cohesion: 0.5333333333333333
        health_score: 60
        message: Good overall cohesion with room for improvement
    ```

=== "HTML"

    The HTML plot output includes two charts:

    1. **Function Cohesion Scores** — A bar chart showing per-function cohesion scores, sorted worst-first. Color-coded: green (≥0.6), yellow (0.4–0.6), orange (0.3–0.4), red (<0.3). Includes reference lines at threshold boundaries.

    2. **Cohesion Distribution** — A pie chart showing the distribution of functions across quality categories (Excellent, Good, Fair, Poor).

    The raw report behind the plots lists every function with its
    `start_line`, `line_count` (the function's line span), `variable_count`
    (distinct variables), `cohesion`, the three assessments and
    `_source_file`.

---

## See also

- [Understanding cohesion metrics](../explanation/cohesion.md) — the mental model, variable rules, formulas, per-language notes, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

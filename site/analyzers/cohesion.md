# Cohesion analyzer reference

The cohesion analyzer computes **LCOM-HS (Henderson-Sellers)** and **variable
sharing ratio** metrics to identify files and modules with low internal
cohesion.

For the conceptual model — what LCOM-HS measures and how to read the scores —
see [Understanding cohesion metrics](../explanation/cohesion.md). To run it,
see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The cohesion analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example output

=== "Text"

    ```
    ╭──────────────────────────────────────────────╮
    │                   COHESION                    │
    │               Score: 7/10                     │
    │  Good cohesion - functions have reasonable    │
    │  focus                                        │
    ╰──────────────────────────────────────────────╯

    Key Metrics
      Total Functions ....... 5
      LCOM Score ............ 0.30
      Cohesion Score ........ 0.70
      Avg Cohesion .......... 0.65

    Distribution
      Excellent (>0.6) ..... 40%  (2)
      Good (0.4-0.6) ....... 20%  (1)
      Fair (0.3-0.4) ....... 20%  (1)
      Poor (<0.3) .......... 20%  (1)

    Issues (sorted worst-first)
      isolatedHelper    0.10  Poor
      utilFunction      0.35  Fair
      parseInput        0.45  Good
    ```

=== "JSON"

    ```json
    {
      "function_cohesion": [
        {
          "name": "isolatedHelper",
          "cohesion": 0.1,
          "quality_level": "Poor"
        },
        {
          "name": "utilFunction",
          "cohesion": 0.35,
          "quality_level": "Fair"
        },
        {
          "name": "parseInput",
          "cohesion": 0.45,
          "quality_level": "Good"
        },
        {
          "name": "processData",
          "cohesion": 0.8,
          "quality_level": "Excellent"
        },
        {
          "name": "handleRequest",
          "cohesion": 0.9,
          "quality_level": "Excellent"
        }
      ],
      "distribution": {
        "excellent": 2,
        "good": 1,
        "fair": 1,
        "poor": 1
      },
      "low_cohesion_functions": [
        {
          "name": "isolatedHelper",
          "cohesion": 0.1,
          "risk_level": "HIGH",
          "recommendation": "Consider splitting into multiple focused functions"
        },
        {
          "name": "utilFunction",
          "cohesion": 0.35,
          "risk_level": "MEDIUM",
          "recommendation": "Review function responsibilities for possible separation"
        }
      ],
      "aggregate": {
        "total_functions": 5,
        "lcom": 0.3,
        "lcom_variant": "LCOM-HS (Henderson-Sellers)",
        "cohesion_score": 0.7,
        "function_cohesion": 0.52,
        "health_score": 70,
        "message": "Good cohesion - functions have reasonable focus"
      }
    }
    ```

=== "HTML"

    The HTML plot output includes two charts:

    1. **Function Cohesion Scores** — A bar chart showing per-function cohesion scores, sorted worst-first. Color-coded: green (≥0.6), yellow (0.4–0.6), orange (0.3–0.4), red (<0.3). Includes reference lines at threshold boundaries.

    2. **Cohesion Distribution** — A pie chart showing the distribution of functions across quality categories (Excellent, Good, Fair, Poor).

---

## See also

- [Understanding cohesion metrics](../explanation/cohesion.md) — the mental model, formulas, references, and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

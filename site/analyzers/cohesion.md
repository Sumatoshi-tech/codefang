# Cohesion Analyzer

The cohesion analyzer computes **LCOM-HS (Henderson-Sellers)** and **variable sharing ratio** metrics to identify files and modules with low internal cohesion. Low cohesion indicates functions that are poorly related to each other — a strong signal for refactoring.

---

## Quick Start

```bash
uast parse main.go | codefang analyze -a cohesion
```

Or analyze an entire directory:

```bash
codefang analyze -a cohesion ./src/
```

---

## What It Measures

### LCOM-HS (Lack of Cohesion of Methods — Henderson-Sellers)

LCOM-HS is the industry-standard cohesion metric used by NDepend, JArchitect, and CppDepend.

**Formula:**

```
LCOM = 1 - sum(mA) / (m × a)
```

Where:

- `m` = number of functions in the file
- `a` = number of distinct variables across all functions
- `mA` = for each variable, the count of functions that reference it
- `sum(mA)` = sum of access counts across all variables

**Range:** 0.0 (perfect cohesion) to 1.0 (no cohesion).

!!! info "Interpretation"
    - **LCOM ≤ 0.3** (green): Excellent cohesion — functions share most variables.
    - **LCOM ≤ 0.6** (yellow): Moderate cohesion — some functions may be loosely related.
    - **LCOM > 0.6** (red): Poor cohesion — functions share few variables and may belong in separate files.

### Cohesion Score

A convenience inversion of LCOM-HS: `cohesion_score = 1.0 - LCOM`. Higher is better.

| Cohesion Score | Assessment |
|---|---|
| ≥ 0.7 | Excellent |
| ≥ 0.4 | Good |
| ≥ 0.3 | Fair |
| < 0.3 | Poor |

### Function-Level Cohesion (Variable Sharing Ratio)

For each function, the analyzer computes what fraction of its variables are shared with at least one other function in the same file. This measures **communicational cohesion** (Yourdon-Constantine level 5).

**Formula:**

```
function_cohesion = shared_vars / total_unique_vars
```

Where:

- `shared_vars` = variables in this function that also appear in at least one other function
- `total_unique_vars` = total distinct variables in this function
- Functions with no variables receive a score of 1.0 (trivial, no penalty)

**Range:** 0.0 (completely isolated function) to 1.0 (all variables shared).

| Function Cohesion | Assessment |
|---|---|
| ≥ 0.6 | Excellent — function shares most variables with the module |
| ≥ 0.4 | Good — reasonable sharing with room for improvement |
| ≥ 0.3 | Fair — consider refactoring |
| < 0.3 | Poor — function is isolated from the module |

---

## Configuration Options

The cohesion analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

---

## Example Output

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

## Use Cases

- **Refactoring targets**: Find functions with low sharing ratio that do unrelated work and should be split.
- **Architecture reviews**: Validate that files follow the Single Responsibility Principle by checking LCOM-HS scores.
- **Code quality tracking**: Monitor cohesion trends over time to catch degradation.
- **Code review**: Use per-function cohesion scores to identify newly added functions that lack cohesion with existing code.

---

## Methodology References

- **LCOM-HS**: Henderson-Sellers, B. (1996). *Object-Oriented Metrics: Measures of Complexity*. Prentice Hall. The Henderson-Sellers variant normalizes LCOM to [0, 1] and is the variant used by NDepend, JArchitect, and CppDepend.
- **Variable Sharing Ratio**: Measures communicational cohesion per the Yourdon-Constantine classification. A function that shares all its variables with other functions in the module exhibits high communicational cohesion.

---

## Limitations

- **Language scope**: Works with any language supported by the UAST. Best results with languages that have explicit variable declarations and function definitions.
- **Accessor methods**: Simple getters/setters may inflate cohesion since each touches only one shared field.
- **Variable naming**: The analyzer uses lexical variable names. Different variables with the same name across functions will be counted as shared.
- **Single-function files**: Files with only one function always receive perfect cohesion (LCOM = 0.0, cohesion = 1.0) since there are no other functions to compare against.
- **Trivial functions**: Functions with no variables receive a cohesion score of 1.0 to avoid penalizing simple utility functions.

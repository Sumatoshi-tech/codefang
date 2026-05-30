# Understanding cohesion metrics

This page explains the mental model behind the cohesion analyzer: what
LCOM-HS measures, how the variable-sharing ratio captures communicational
cohesion, and how to read the scores. For configuration keys and the output
schema, see the [Cohesion reference](../analyzers/cohesion.md).

---

## What it measures

The cohesion analyzer computes **LCOM-HS (Henderson-Sellers)** and **variable
sharing ratio** metrics to identify files and modules with low internal
cohesion. Low cohesion indicates functions that are poorly related to each
other — a strong signal for refactoring.

### LCOM-HS (lack of cohesion of methods — Henderson-Sellers)

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

### Cohesion score

A convenience inversion of LCOM-HS: `cohesion_score = 1.0 - LCOM`. Higher is better.

| Cohesion Score | Assessment |
|---|---|
| ≥ 0.7 | Excellent |
| ≥ 0.4 | Good |
| ≥ 0.3 | Fair |
| < 0.3 | Poor |

### Function-level cohesion (variable sharing ratio)

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

## Use cases

- **Refactoring targets**: Find functions with low sharing ratio that do unrelated work and should be split.
- **Architecture reviews**: Validate that files follow the Single Responsibility Principle by checking LCOM-HS scores.
- **Code quality tracking**: Monitor cohesion trends over time to catch degradation.
- **Code review**: Use per-function cohesion scores to identify newly added functions that lack cohesion with existing code.

---

## Methodology references

- **LCOM-HS**: Henderson-Sellers, B. (1996). *Object-Oriented Metrics: Measures of Complexity*. Prentice Hall. The Henderson-Sellers variant normalizes LCOM to [0, 1] and is the variant used by NDepend, JArchitect, and CppDepend.
- **Variable Sharing Ratio**: Measures communicational cohesion per the Yourdon-Constantine classification. A function that shares all its variables with other functions in the module exhibits high communicational cohesion.

---

## Limitations

- **Language scope**: Works with any language supported by the UAST. Best results with languages that have explicit variable declarations and function definitions.
- **Accessor methods**: Simple getters/setters may inflate cohesion since each touches only one shared field.
- **Variable naming**: The analyzer uses lexical variable names. Different variables with the same name across functions will be counted as shared.
- **Single-function files**: Files with only one function always receive perfect cohesion (LCOM = 0.0, cohesion = 1.0) since there are no other functions to compare against.
- **Trivial functions**: Functions with no variables receive a cohesion score of 1.0 to avoid penalizing simple utility functions.

---

## See also

- [Cohesion reference](../analyzers/cohesion.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run your first analysis.

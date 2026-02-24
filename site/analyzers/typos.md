# Typos Analyzer

The typos analyzer detects **typo-fix identifier pairs** from source code in commit diffs using Levenshtein distance. It builds a dataset of probable typos and their corrections by analyzing UAST identifier changes across Git history.

---

## Quick Start

```bash
codefang run -a history/typos .
```

With a custom distance threshold:

```bash
codefang run -a history/typos --typos-max-distance 3 .
```

!!! note "Requires UAST"
    The typos analyzer needs UAST support to extract identifiers from source code. It is automatically enabled when the UAST pipeline is available.

---

## Architecture

The typos analyzer follows the **TC/Aggregator pattern**:

1. **Consume phase**: For each commit, `Consume()` computes diffs, identifies line pairs within Levenshtein distance, and extracts UAST identifier changes. Per-commit typos are returned as `TC{Data: []Typo}`. The analyzer retains no per-commit state; only the `lcontext` (Levenshtein context) is kept as working state.
2. **Aggregation phase**: A `typos.Aggregator` collects TCs into a `SliceSpillStore[Typo]`. `FlushTick()` deduplicates typos by `wrong|correct` key (keeping the first occurrence), returning a `TickData` with the unique set.
3. **Serialization phase**: `SerializeTICKs()` assembles all tick data into an `analyze.Report{"typos": allTypos}`, then delegates to `ComputeAllMetrics()` for JSON, YAML, binary, or HTML plot output.

This separation enables streaming output, budget-aware memory spilling, and decoupled aggregation.

---

## What It Measures

### Typo-Fix Pair Detection

For each commit, the analyzer:

1. Computes the diff between the old and new versions of each changed file
2. Identifies delete/insert hunk pairs of equal size (same number of lines)
3. Compares corresponding lines using **Levenshtein distance**
4. For line pairs within the distance threshold, extracts UAST identifiers from the old and new versions
5. If exactly one identifier changed between the two versions, it records a typo-fix pair

### Levenshtein Distance

The Levenshtein distance is the minimum number of single-character edits (insertions, deletions, substitutions) needed to transform one string into another. A small distance between the old and new line suggests a typo fix rather than a semantic change.

!!! info "Example"
    - `recieve` to `receive` -- distance 2 (transposed characters)
    - `lenght` to `length` -- distance 2 (transposed characters)
    - `calcualte` to `calculate` -- distance 2 (transposed characters)

---

## Configuration Options

| Option | Type | Default | Description |
|---|---|---|---|
| `TyposDatasetBuilder.MaximumAllowedDistance` | `int` | `4` | Maximum Levenshtein distance between two lines to consider them a typo-fix candidate. Lower values produce fewer but higher-confidence results. |

```yaml
# .codefang.yml
history:
  typos:
    max_distance: 4
```

!!! tip "Tuning the distance"
    - **Distance 1-2**: Very high confidence. Catches single-character typos.
    - **Distance 3-4** (default): Good balance. Catches transposition errors and short misspellings.
    - **Distance 5+**: Lower confidence. May produce false positives from intentional renames.

---

## Example Output

=== "JSON"

    ```json
    {
      "typos": [
        {
          "wrong": "recieve",
          "correct": "receive",
          "file": "pkg/api/handler.go",
          "commit": "a1b2c3d4e5f6...",
          "line": 42
        },
        {
          "wrong": "calcualte",
          "correct": "calculate",
          "file": "pkg/math/stats.go",
          "commit": "f6e5d4c3b2a1...",
          "line": 15
        },
        {
          "wrong": "reponse",
          "correct": "response",
          "file": "pkg/api/client.go",
          "commit": "1a2b3c4d5e6f...",
          "line": 88
        }
      ]
    }
    ```

=== "YAML"

    ```yaml
    typos:
      - wrong: recieve
        correct: receive
        file: pkg/api/handler.go
        line: 42
      - wrong: calcualte
        correct: calculate
        file: pkg/math/stats.go
        line: 15
      - wrong: reponse
        correct: response
        file: pkg/api/client.go
        line: 88
    ```

---

## Use Cases

- **Typo dataset building**: Build a corpus of real-world typos and their corrections from your project's history. This can train spell-checking tools or IDE plugins.
- **Code quality auditing**: Identify patterns of common misspellings in your codebase to add to a linting dictionary.
- **API consistency**: Detect identifier typos that may cause confusion (e.g., `getUserByNmae` vs `getUserByName`).
- **Automated fix suggestions**: Use the typo dataset to build automated correction rules for CI pipelines.
- **Research**: Academic research on developer typo patterns and their prevalence across different languages.

---

## Limitations

- **UAST required**: Only languages with UAST parser support are analyzed. Identifiers in unsupported languages are not extracted.
- **Single-identifier changes only**: The analyzer only records a typo when exactly one identifier changes between the old and new lines. Multi-identifier changes are skipped to avoid false positives.
- **Equal-length hunks only**: Only delete/insert hunk pairs with the same number of lines are considered. A typo fix that also adds or removes lines will be missed.
- **False positives**: Intentional identifier renames with small Levenshtein distance (e.g., `idx` to `jdx`) will be reported as typos.
- **Deduplication**: Typo pairs are deduplicated by the `wrong|correct` key. The same typo fixed in multiple commits is reported only once.
- **CPU intensive**: Like all UAST-based analyzers, the typos analyzer parses both file versions for every changed file in every commit.

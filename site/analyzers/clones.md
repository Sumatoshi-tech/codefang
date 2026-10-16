# Clone detection analyzer reference

The clone detection analyzer (`static/clones`) finds **duplicate and
near-duplicate functions** across your entire codebase using MinHash
signatures and Locality-Sensitive Hashing (LSH). It operates on the UAST
representation and detects clones **cross-file** — not just within a single
file.

For the conceptual model — the clone-type taxonomy, the methodology, and why
MinHash + LSH — see
[Understanding clone detection](../explanation/clones.md). To run it, see the
[Quick start](../getting-started/quickstart.md).

---

## Configuration options

The clone detection analyzer uses the UAST directly and has no analyzer-specific configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

The walk honors the run-wide flags: `--languages` selects the files to parse,
and `--include-vendored` / `--include-generated` add vendored / generated
files to the analysis.

---

## Clone types

| Type | `similarity` | Rule |
|---|---|---|
| **Type-1** (exact) | `1.00` | identical AST structure **and** identical tokens |
| **Type-2** (renamed) | `1.00` | identical AST structure; identifiers and/or literals differ |
| **Type-3** (near-miss) | `0.50` … `< 1.00` (or `1.00` with a reordered body) | modified structure whose exact shingle Jaccard is ≥ 0.5 |

`similarity` is the **exact Jaccard index of the two functions' shingle
sets** (not a MinHash estimate); it is `1.0` whenever the structure is
identical. Comments never affect the structure or the tokens, so a copy that
only gained comments is still Type-1.

---

## Metrics

| Metric | Definition |
|---|---|
| `total_functions` | every function, method and closure (UAST `Lambda`) found, including ones too small to pair |
| `total_clone_pairs` | the exact number of pairs found (not capped) |
| `clone_ratio` | **functions that take part in at least one pair / `total_functions`** — bounded to `[0, 1]`; e.g. 3 functions where two are copies of each other → `2 / 3 = 0.67` |
| `clone_type_distribution` | exact per-type pair counts |
| `clone_pairs` | the stored detail: the best 1000 pairs ordered by similarity descending, then by `func_a`, `func_b` |

The status message and the `N/10` score are **both keyed off `clone_ratio`**:

| `clone_ratio` | Status | Score |
|---|---|---|
| `0` | `No code clones detected` | `10/10` |
| `≤ 0.1` | `Low duplication - few clone pairs detected` | `round((1 − ratio) × 10)/10` |
| `≤ 0.3` | `Moderate duplication - consider refactoring clone pairs` | ″ |
| `> 0.3` | `High duplication - significant refactoring recommended` | ″ |

Two files that are copies of each other have a ratio of `1.0`: `High
duplication`, `0/10`.

### Candidates and minimum size

- Every function, method and closure is counted. A closure is named by its
  enclosing function and ordinal (`Handle.func#2`), or `func#N` at file level.
- A function must have at least **20 UAST nodes** (comments excluded) to
  become a clone candidate; smaller ones (one-line getters, `return a + b`)
  are counted in `total_functions` but never paired, because they hash
  identically regardless of purpose.
- A closure is never paired with the function it is written in.

---

## Example output

=== "Text"

    ```
    ┃ CLONE DETECTION                                                  Score: 3/10 ┃

      Summary: High duplication - significant refactoring recommended

      Key Metrics
      Total Functions     4           Clone Pairs         2
      Clone Ratio         0.8

      Distribution
      Type-1 (Exact)     ████████████████████░░░░░░░░░░░░░░░░░░░░  50%  (1)
      Type-2 (Renamed)   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   0%  (0)
      Type-3 (Near-miss) ████████████████████░░░░░░░░░░░░░░░░░░░░  50%  (1)

      Top Issues
      a.go::ProcessItems <-> b.go::ProcessItems       Type-1              1.00
      a.go::ProcessItems <-> d.go::ProcessItemsPlus   Type-3              0.63
    ```

=== "JSON"

    `--format json` emits the sections envelope (the shape every static
    analyzer shares):

    ```json
    {
      "overall_score_label": "3/10",
      "sections": [
        {
          "title": "CLONE DETECTION",
          "score_label": "3/10",
          "status": "High duplication - significant refactoring recommended",
          "metrics": [
            { "label": "Total Functions", "value": "4" },
            { "label": "Clone Pairs", "value": "2" },
            { "label": "Clone Ratio", "value": "0.8" }
          ],
          "distribution": [
            { "label": "Type-1 (Exact)", "percent": 0.5, "count": 1 },
            { "label": "Type-2 (Renamed)", "percent": 0, "count": 0 },
            { "label": "Type-3 (Near-miss)", "percent": 0.5, "count": 1 }
          ],
          "issues": [
            { "name": "a.go::ProcessItems <-> b.go::ProcessItems", "location": "Type-1", "value": "1.00", "severity": "poor" },
            { "name": "a.go::ProcessItems <-> d.go::ProcessItemsPlus", "location": "Type-3", "value": "0.63", "severity": "fair" }
          ],
          "score": 0.25
        }
      ],
      "overall_score": 0.25
    }
    ```

    `distribution[].percent` is a fraction in `0..1` (the renderer contract
    shared by every analyzer); `issues[].location` carries the clone type,
    `issues[].value` the similarity with two decimals, and `severity` is
    `poor` at similarity ≥ 0.8, `fair` below.

=== "YAML / bin"

    `--format yaml` and `--format bin` carry the computed metrics with the
    full stored pair list:

    ```yaml
    total_functions: 4
    total_clone_pairs: 2
    clone_ratio: 0.75
    clone_type_distribution:
        Type-1: 1
        Type-2: 0
        Type-3: 1
    clone_pairs:
        - func_a: a.go::ProcessItems
          func_b: b.go::ProcessItems
          similarity: 1
          clone_type: Type-1
        - func_a: a.go::ProcessItems
          func_b: d.go::ProcessItemsPlus
          similarity: 0.6349206349206349
          clone_type: Type-3
    message: High duplication - significant refactoring recommended
    ```

=== "HTML"

    The HTML plot output includes a **Clone Type Distribution** pie chart showing the breakdown of detected clones by type (Type-1, Type-2, Type-3).

---

## `--per-file`

Clone detection is cross-file, so a per-file view answers "which of this
file's functions are duplicated elsewhere?". With `--per-file` the section
gains a `files` array with one entry per parsed file, in walk order:

```json
{
  "file_path": "a.go",
  "score_label": "0/10",
  "status": "1 of 1 functions in clone pairs",
  "metrics": [
    { "label": "Total Functions", "value": "1" },
    { "label": "Cloned Functions", "value": "1" },
    { "label": "Clone Ratio", "value": "1.0" }
  ],
  "issues": [
    { "name": "a.go::ProcessItems <-> b.go::ProcessItems", "location": "Type-1", "value": "1.00", "severity": "poor" }
  ],
  "score": 0
}
```

`Cloned Functions` counts the file's functions that take part in **any**
pair; `issues` lists the stored pairs (the best 1000 overall) that involve the
file. A file whose functions are not duplicated reads `No functions in clone
pairs`; a file without functions reads `No functions found for clone
analysis`.

---

## See also

- [Understanding clone detection](../explanation/clones.md) — the mental model, methodology, references, and limitations.
- [Composition](composition.md) — the other cross-file static analyzer.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

# Typos analyzer reference

The typos analyzer detects **typo-fix identifier pairs** from source code in
commit diffs using Levenshtein distance.

For the conceptual model — how detection works and what Levenshtein distance
measures — see [Understanding typo detection](../explanation/typos.md). To run
it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

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

The analyzer requires UAST support to extract identifiers from source code. It is automatically enabled when the UAST pipeline is available. The `--typos-max-distance` CLI flag sets `MaximumAllowedDistance` for a single run.

---

## Example output

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

## See also

- [Understanding typo detection](../explanation/typos.md) — the mental model, Levenshtein distance, architecture, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

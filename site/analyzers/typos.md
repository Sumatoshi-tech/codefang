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
| `TyposDatasetBuilder.MaximumAllowedDistance` | `int` | `4` | Maximum Levenshtein distance between the two identifiers of a typo-fix pair (also the bound on the whole-line prefilter). Lower values produce fewer but higher-confidence results. |

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

The analyzer requires UAST support to extract identifiers from source code: only files the parser supports are examined. The `--typos-max-distance` CLI flag sets `MaximumAllowedDistance` for a single run.

---

## What counts as a typo fix

A typo fix is **one identifier that a commit changed into a different,
Levenshtein-close identifier on the same line**:

- the removed and the added line are aligned (equal-sized delete/insert
  runs) and are within the distance bound of each other as whole lines;
- the identifiers the two lines share cancel, whatever their position
  (`v := recieve()` → `v := receive()` keeps `v` out of the way);
- exactly one identifier remains on each side, both non-empty and
  different, and the two are within the distance bound.

Not reported: a literal change next to an unchanged identifier
(`#define N 4` → `#define N 17`), a rename to something unrelated
(`foo` → `completelyDifferent`), a line whose identifiers changed in more
than one place, prose in files the parser does not support.

---

## Example output

```bash
codefang run -a history/typos --format json ./path/to/repo
```

=== "JSON"

    ```json
    {
      "aggregate": {
        "total_typos": 2,
        "unique_patterns": 1,
        "affected_files": 1,
        "affected_commits": 1
      },
      "file_typos": [
        { "file": "a.go", "typo_count": 2, "fixed_typos": 2 }
      ],
      "patterns": [
        { "wrong": "recieve", "correct": "receive", "frequency": 2 }
      ],
      "typo_list": [
        {
          "wrong": "recieve",
          "correct": "receive",
          "file": "a.go",
          "line": 3,
          "commit": "a1b2c3d4e5f6..."
        }
      ]
    }
    ```

=== "YAML"

    ```yaml
    codefang (v2):
      version: ...
      hash: ...
    history/typos:
      aggregate:
        total_typos: 2
        unique_patterns: 1
        affected_files: 1
        affected_commits: 1
      file_typos:
        - file: a.go
          typo_count: 2
          fixed_typos: 2
      patterns:
        - wrong: recieve
          correct: receive
          frequency: 2
      typo_list:
        - wrong: recieve
          correct: receive
          file: a.go
          line: 3
          commit: a1b2c3d4e5f6...
    ```

The report is one value for every machine format: `json` marshals it
compactly, `yaml` renders it under the `history/typos:` header, `bin`
wraps the json bytes in the CFB1 envelope.

---

## Output structure

| Key | Description |
|---|---|
| `typo_list` | Every distinct `wrong → correct` pair once, attributed to its chronologically first fix: `file`, **1-based** `line` in the fixed file, `commit`. The order is first-seen order. |
| `patterns` | The pairs fixed more than once, with the number of occurrences (`frequency`), most frequent first (ties by `wrong`, then `correct`). |
| `file_typos` | Occurrences per file (`typo_count`; `fixed_typos` is the same number — every detected occurrence is a fix), most affected first (ties by file name). |
| `aggregate` | `total_typos` (occurrences), `unique_patterns` (distinct pairs), `affected_files`, `affected_commits`. |

Every list is present and `[]` when empty. A pair fixed on two lines of
one file is one `typo_list` entry, one `patterns` entry with `frequency: 2`,
`typo_count: 2` for the file and `total_typos: 2`.

The per-commit `ndjson` records carry the occurrences of each commit
(`Wrong`, `Correct`, `File`, `Commit` as 20 bytes, 1-based `Line`).

The output is a pure function of the repository and the flags: the same
bytes under any CPU count.

---

## See also

- [Understanding typo detection](../explanation/typos.md) — the mental model, Levenshtein distance, architecture, and limitations.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

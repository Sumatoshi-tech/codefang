# Composition analyzer reference

The composition analyzer (`static/composition`) classifies **every file** in
a tree into one of nine categories — source, documentation, configuration,
vendor, generated, dotfile, image, binary, other — and reports the counts
and shares. It is a raw-file analyzer: it reads each file's path and first
8 KiB, never parses, and therefore sees files the UAST analyzers skip.

It is informational: its score is always `Info`.

---

## What it counts

The analyzer classifies every regular file the walk visits (directories are
recursed, `.git` is skipped). **Vendored and generated files are always
counted** — reporting their share is what the analyzer is for — so the
`--include-vendored` / `--include-generated` flags do not change its
output. The only filter that applies is `--languages`, which narrows the
walk to the selected languages' files:

```bash
codefang run -a static/composition .                 # every file
codefang run -a static/composition --languages go .  # only Go files, still classified into every category
```

---

## Categories and classification rules

Each file is classified against its **root-relative path** and its content
header by a fixed-priority cascade; the first matching rule wins:

| Order | Category | Rule |
|---|---|---|
| 1 | `binary` | non-empty content with a NUL byte in the first 8000 bytes |
| 2 | `image` | extension is exactly `.png`, `.jpg`, `.jpeg` or `.gif` (case-sensitive) |
| 3 | `vendor` | the path matches enry's vendor table (`vendor/`, `node_modules/`, `third_party/`, `testdata/`, minified bundles, …) |
| 4 | `generated` | generated name (`.pb.go`, `.deepcopy.go`, `_string.go`, `zz_generated*`, `mock_*`, `.min.js`, …) or a generated marker (`Code generated`, `DO NOT EDIT`, `@generated`, …) in the first 512 bytes |
| 5 | `documentation` | enry's documentation paths (`docs/`, `Documentation/`, `examples/`, `man/`, `README*`, `LICENSE*`, `CHANGELOG*`, …) or a prose extension (`.md`, `.rst`, `.txt`, `.adoc`, `.org`, …) anywhere |
| 6 | `configuration` | the extension's language is XML, JSON, TOML, YAML or INI |
| 7 | `dotfile` | the base name starts with `.` |
| 8 | `source` | enry recognizes a language from the file name, extension, shebang or modeline (`.go`, `Makefile`, `Dockerfile`, `#!/usr/bin/env python3`, …) |
| 9 | `other` | nothing above matched (unknown extension, no recognizable language) |

Because the order is fixed, `vendor/README.md` is `vendor`, `.golangci.yml`
is `configuration`, `docs/x.pb.go` is `generated`, and a `.gitignore` is a
`dotfile`.

!!! note "`.sql` files"
    enry lists `.sql` under PL/SQL first, so SQL files are `source`, not
    `configuration`. Data formats enry treats as languages (CSV, …) are
    `source` as well; only the five configuration languages above are
    `configuration`.

---

## Output

### Section

| Field | Meaning |
|---|---|
| `status` | `File composition analysis completed`, or `No files analyzed` for an empty tree |
| `metrics` | `Total Files`, `Source Files`, `Source %` |
| `distribution` | one row per non-empty category in the order above: `label`, `percent` (a **fraction in `0..1`**, the shared renderer contract), `count`; omitted when no file was analyzed |
| `issues` | one row per non-empty non-source category: `value` = `"<count> files (<pct>%)"`, `severity` = `poor` for `binary`, `info` otherwise |
| `score` / `score_label` | `-1` / `Info` |

### `--per-file`

With `--per-file` the section gains a `files` array with one entry per
classified file, in walk order:

```json
{
  "file_path": "vendor/x/y.go",
  "score_label": "Info",
  "status": "Classified as vendor",
  "metrics": [
    { "label": "Total Files", "value": "1" },
    { "label": "Vendor Files", "value": "1" },
    { "label": "Vendor %", "value": "100.0%" },
    { "label": "Language", "value": "Go" }
  ],
  "distribution": [ { "label": "vendor", "percent": 1, "count": 1 } ],
  "issues": [],
  "score": -1
}
```

`Language` is the enry language of the file, or `unknown`.

---

## Example output

=== "Text"

    ```
    ┃ COMPOSITION                                                      Score: Info ┃

      Summary: File composition analysis completed

      Key Metrics
      Total Files         13          Source Files        3
      Source %            23.1%

      Distribution
      source             █████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  23%  (3)
      documentation      ██████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  15%  (2)
      configuration      ██████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  15%  (2)
      vendor             ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   7%  (1)
      generated          ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   7%  (1)
      dotfile            ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   7%  (1)
      image              ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   7%  (1)
      binary             ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   7%  (1)
      other              ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   7%  (1)
    ```

=== "JSON"

    `--format json` emits the sections envelope:

    ```json
    {
      "overall_score_label": "Info",
      "sections": [
        {
          "title": "COMPOSITION",
          "score_label": "Info",
          "status": "File composition analysis completed",
          "metrics": [
            { "label": "Total Files", "value": "13" },
            { "label": "Source Files", "value": "3" },
            { "label": "Source %", "value": "23.1%" }
          ],
          "distribution": [
            { "label": "source", "percent": 0.23076923076923078, "count": 3 },
            { "label": "vendor", "percent": 0.07692307692307693, "count": 1 },
            { "label": "other", "percent": 0.07692307692307693, "count": 1 }
          ],
          "issues": [
            { "name": "vendor", "location": "", "value": "1 files (7.7%)", "severity": "info" },
            { "name": "binary", "location": "", "value": "1 files (7.7%)", "severity": "poor" }
          ],
          "score": -1
        }
      ],
      "overall_score": -1
    }
    ```

=== "YAML / bin"

    `--format yaml` and `--format bin` carry the raw aggregate — every
    category, zero counts included, percentages in `0..100`:

    ```yaml
    breakdown:
        binary: 1
        configuration: 2
        documentation: 2
        dotfile: 1
        generated: 1
        image: 1
        other: 1
        source: 3
        vendor: 1
    percentages:
        binary: 7.6923076923076925
        configuration: 15.384615384615385
        documentation: 15.384615384615385
        dotfile: 7.6923076923076925
        generated: 7.6923076923076925
        image: 7.6923076923076925
        other: 7.6923076923076925
        source: 23.076923076923077
        vendor: 7.6923076923076925
    total_files: 13
    ```

---

## Configuration options

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | No analyzer-specific config; `--languages` narrows the walk |

---

## See also

- [Clones](clones.md) — the other cross-file static analyzer.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

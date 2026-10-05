# Comments analyzer reference

The comments analyzer (`static/comments`) measures **documentation coverage**
and **comment placement** across your source code: which declarations carry a
doc comment, where every comment sits, and what that adds up to per file and
per repository.

For the mental model — what "documented" means, the placement categories and
the gap rule — see [Understanding documentation coverage](../explanation/comments.md).
To run it, see the [Quick start](../getting-started/quickstart.md).

---

## Configuration options

The comments analyzer uses the UAST directly and has no analyzer-specific
configuration options.

| Option | Type | Default | Description |
|---|---|---|---|
| *(none)* | -- | -- | Uses UAST; no analyzer-specific config |

The shared static flags apply: `--per-file` adds one entry per analyzed file
to the JSON report, `--languages` and the path policy flags select the files.

---

## Definitions

| Term | Definition |
|---|---|
| **Declaration** | A UAST `Function`, `Method`, `Class`, `Interface` or `Struct` node. A nameless `Class` / `Struct` / `Interface` node (a type literal, a class body) is not a declaration. A declaration without a resolvable name is reported as `unknown`. |
| **Comment** | A UAST `Comment` node. Nested comment pieces under one node count as one comment. Comments on adjacent lines form one **block**. |
| **above** | The block ends on the line directly above a declaration (attribute / decorator lines belong to the declaration). The block **documents** that declaration; every comment in it is **good**. |
| **docstring** | The block is a documentation comment (UAST `Doc` role) that is the first statement of a declaration body — a Python docstring. It **documents** that declaration; **good**. A string literal later in the body is **inline**. |
| **inline** | The block lies inside a declaration's span (a body comment). Attributed to the innermost declaration; **bad**. |
| **unassociated** | Any other block (file headers, orphans, a comment separated from the declaration by a blank line); **bad**. |
| **Doc Coverage** | documented declarations / all declarations — summed over every analyzed file (Σ/Σ). |
| **Good Ratio** | good comments / all comments — summed over every analyzed file (Σ/Σ). |
| **Score** | The Good Ratio, shown as `N/10`. |
| **Status** | Threshold label of the score: `>= 0.8` Excellent, `>= 0.6` Good, `>= 0.4` Fair, else Poor. With neither comments nor declarations: `No comments found`. |
| **Risk level** (yaml / bin) | Length of an undocumented declaration: `HIGH` at 50 lines or more, `MEDIUM` at 10 or more, else `LOW`. |

A file the parser supports but cannot parse (for example Markdown) is listed
as an empty file: it has no declarations and no comments, so it never changes
a ratio.

---

## Example output

Fixture: `a.go` has `Add` (documented) and `Sub`, plus a package comment and a
body comment; `b.go` has `Mul` (its comment is followed by a blank line), `Div`
(documented by a block comment) and `Mod`; `README.md` is prose.

=== "JSON"

    ```json
    {
      "overall_score_label": "4/10",
      "sections": [
        {
          "title": "COMMENTS",
          "score_label": "4/10",
          "status": "Fair comment quality - consider improving placement",
          "metrics": [
            {"label": "Total Comments", "value": "5"},
            {"label": "Good Comments", "value": "2"},
            {"label": "Bad Comments", "value": "3"},
            {"label": "Doc Coverage", "value": "40.0%"},
            {"label": "Good Ratio", "value": "40.0%"},
            {"label": "Total Functions", "value": "5"}
          ],
          "distribution": [
            {"label": "Documented", "percent": 0.4, "count": 2},
            {"label": "Undocumented", "percent": 0.6, "count": 3}
          ],
          "issues": [
            {"name": "Mod", "location": "b.go", "value": "undocumented", "severity": "poor"},
            {"name": "Mul", "location": "b.go", "value": "undocumented", "severity": "poor"},
            {"name": "Sub", "location": "a.go", "value": "undocumented", "severity": "poor"}
          ],
          "score": 0.4
        }
      ],
      "overall_score": 0.4
    }
    ```

    Issues are sorted by name, then file, then line. `distribution` is
    omitted when there are no declarations.

    With `--per-file`, a `files` array sits between `issues` and `score`,
    one entry per analyzed file (empty files included), in walk order:

    ```json
    {
      "file_path": "b.go",
      "score_label": "5/10",
      "status": "Fair comment quality - consider improving placement",
      "metrics": [ ... the same six metrics for this file ... ],
      "distribution": [
        {"label": "Documented", "percent": 0.3333333333333333, "count": 1},
        {"label": "Undocumented", "percent": 0.6666666666666666, "count": 2}
      ],
      "issues": [
        {"name": "Mod", "location": "b.go", "value": "undocumented", "severity": "poor"},
        {"name": "Mul", "location": "b.go", "value": "undocumented", "severity": "poor"}
      ],
      "score": 0.5
    }
    ```

    `file_path` and every `location` are relative to the analyzed root
    (`sub/c.go`); a single-file root stamps the file name.

=== "YAML"

    ```yaml
    comment_quality:
        - line_number: 4
          source_file: a.go
          language: go
          directory: .
          quality: good          # good | bad
          placement: above       # above | docstring | inline | unassociated
          target_type: Function
          target_name: Add
          score: 1
        - line_number: 6
          source_file: a.go
          language: go
          directory: .
          quality: bad
          placement: inline
          target_type: Function
          target_name: Add
          score: -0.5
    function_documentation:
        - name: Add
          type: Function
          line: 5
          lines: 4
          source_file: a.go
          language: go
          directory: .
          is_documented: true
          doc_line: 4            # 0 when undocumented
          status: Documented
        - name: Sub
          type: Function
          line: 10
          lines: 3
          source_file: a.go
          language: go
          directory: .
          is_documented: false
          doc_line: 0
          status: Undocumented
    undocumented_functions:
        - name: Sub
          type: Function
          line: 10
          lines: 3
          source_file: a.go
          language: go
          directory: .
          risk_level: LOW        # LOW | MEDIUM | HIGH
    aggregate:
        total_comments: 5
        good_comments: 2
        bad_comments: 3
        overall_score: 0.4
        total_functions: 5
        documented_functions: 2
        good_comments_ratio: 0.4
        documentation_coverage: 0.4
        health_score: 40
        message: Fair comment quality - consider improving placement
    ```

    Rows are sorted by file, then line. `--format bin` carries exactly this
    value in the CFB1 envelope.

=== "Text"

    ```
    COMMENTS                                                     Score: 4/10

      Summary: Fair comment quality - consider improving placement

      Key Metrics
      Total Comments      5           Good Comments       2
      Bad Comments        3           Doc Coverage        40.0%
      Good Ratio          40.0%       Total Functions     5

      Distribution
      Documented         ████████████████                          40%  (2)
      Undocumented       ████████████████████████                  60%  (3)
    ```

    The text and compact formats show the summary without the issue list.

Per-comment scores (`score` in `comment_quality`): `1.0` when above, the
enclosing declaration's penalty when inline (`Function` / `Method` `-0.5`,
`Class` / `Interface` / `Struct` `-0.3`, others `-0.1`), `-0.2` when
unassociated. Only the good / bad classification feeds the ratios.

---

## See also

- [Understanding documentation coverage](../explanation/comments.md) — the mental model, placement categories, the gap rule and limitations.
- [Quick start](../getting-started/quickstart.md) — run static analysis.

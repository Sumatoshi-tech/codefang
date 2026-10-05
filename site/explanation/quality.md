# Understanding code quality over time

This page explains the mental model behind the code quality analyzer: which
static metrics it tracks per commit and how it aggregates them per tick. For
the output schema and the exact per-field definitions, see the
[Quality reference](../analyzers/quality.md).

---

## What it measures

The code quality analyzer answers one question per tick: *what did the code
touched in this period look like?* For every commit in the window it takes
the files the commit changed, parses each one's new version into a UAST, and
runs the four static analyzers on it — exactly the analyzers behind
`static/complexity`, `static/halstead`, `static/comments` and
`static/cohesion`. The per-file numbers are therefore the numbers those
commands print for the same file.

### Per-file sample

For each parsed file in a commit, the analyzer records:

| Category | Metrics |
|---|---|
| **Complexity** | summed cyclomatic complexity, summed cognitive complexity, max single-function complexity, function count |
| **Halstead** | volume, effort, delivered bugs (over the file's functions, with operator and operand tokens recovered from the file bytes) |
| **Comments** | good-comment ratio, documentation coverage |
| **Cohesion** | cohesion score — only for files with at least one function |

The sample is computed by `cf_quality::analyze_file(root, source)`, which
feeds both the UAST and the file bytes to the component analyzers. The one
deliberate difference from the static commands is a discovery depth cap:
functions and comments nested more than ten levels below the file root are
not discovered (each discovered function is analyzed in full).

### Which files count

- A file counts when its parse succeeded. Unsupported languages, binaries,
  blobs above 256 KiB and files whose parse fails contribute nothing — they
  are not "analyzed files" and they do not pad the statistics with zeros.
- A parsed file without functions is an analyzed file: its complexity and
  Halstead scalars are zero (there is nothing to measure) and its comment
  scalars are whatever the comments analyzer reports. It carries **no
  cohesion sample**, mirroring the static cohesion analyzer's "no data".
- Deleted files contribute nothing.

### Per-tick aggregation

Commits are bucketed by committer time into ticks (`--tick-size`, default
24 hours). A tick's statistics are computed over the concatenated samples of
its commits — mean, median, P95 and max for complexity and Halstead volume,
sums for delivered bugs and volume, mean and minimum for the comment score,
mean and minimum for cohesion.

A tick in which nothing was analyzed does not exist in the report: it is not
a tick with zero complexity, it is a tick with no data. The aggregate is
computed over the ticks that do exist, and the cohesion aggregates over the
ticks that hold cohesion data, so an aggregate minimum can never exceed the
matching aggregate mean.

### Large commits

Every surviving change of a commit is analyzed; there is no per-commit file
cap other than `--max-changes-per-commit`, which drops a commit entirely when
its raw tree diff exceeds the limit.

---

## Architecture

1. **Walk**: the shared history core yields the commit window, identity and
   tick stamps; a parallel pre-pass computes each commit's filtered tree diff
   and parses the changed blobs once (one parser per worker thread).
2. **Sample**: `cf_quality::analyze_file` turns each parsed file into a
   `FileSample`; a commit's samples form its `TickQuality`.
3. **Reduce**: samples are merged per tick in chronological order and
   `cf_quality::compute_all_metrics` derives the time series and the
   aggregate. The output is a pure function of the repository and the flags.

---

## See also

- [Quality reference](../analyzers/quality.md) — output schema and field definitions.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

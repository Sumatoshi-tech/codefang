# Understanding typo detection

This page explains the mental model behind the typos analyzer: how it detects
typo-fix identifier pairs, what Levenshtein distance measures, and how the
aggregation pipeline is structured. For configuration keys and the output
schema, see the [Typos reference](../analyzers/typos.md).

---

## What it measures

The typos analyzer detects **typo-fix identifier pairs** from source code in
commit diffs using Levenshtein distance. It builds a dataset of probable typos
and their corrections by analyzing UAST identifier changes across Git history.

### Typo-fix pair detection

For each commit, the analyzer:

1. Computes the line diff between the old and new versions of each changed file (both versions must be text and parse with the UAST)
2. Aligns delete/insert hunk pairs of equal size (line *i* of each)
3. Keeps the aligned line pairs whose whole lines are within the distance threshold — a cheap prefilter that bounds the UAST work
4. Extracts the UAST identifiers of each kept line pair and diffs them as multisets: identifiers present on both lines cancel, whatever their position
5. Records a typo-fix pair when exactly one identifier remains on each side, both are non-empty and different, and the two identifiers are within the distance threshold of each other

The distance is therefore applied twice: to the whole line (prefilter) and to the identifier pair (the rule). A literal change next to an unchanged identifier leaves nothing after cancellation and is never a typo; a rename to an unrelated name fails the pair bound.

### Levenshtein distance

The Levenshtein distance is the minimum number of single-character edits (insertions, deletions, substitutions) needed to transform one string into another. A small distance between the old and new identifier suggests a typo fix rather than a semantic change.

!!! info "Example"
    - `recieve` to `receive` -- distance 2 (transposed characters)
    - `lenght` to `length` -- distance 2 (transposed characters)
    - `calcualte` to `calculate` -- distance 2 (transposed characters)

---

## Architecture

The detection rules and the metrics live in the `cf-typos` crate; the history walk in `cf-commands` supplies the git and UAST inputs:

1. **Per-commit product**: for every Modify change the walk reads both blobs, runs the libgit2 line diff and both UAST parses, and asks `cf_typos::candidate_line_pairs` for the aligned line pairs within the prefilter and `cf_typos::identifier_pair` for the one identifier each pair changed. The product is the commit's list of occurrences (file, 1-based line, commit); it is a pure function of the commit, so it is computed in parallel across cores.
2. **Chronological reduce**: the shared history core stamps every commit with its identity and tick in oldest-first order, and the occurrences are concatenated in that order — the report never depends on how the work was distributed.
3. **Metrics**: `cf_typos::metrics_report_value` derives `typo_list` (each distinct pair once, first fix wins), `patterns` (occurrence counts above one), `file_typos` and `aggregate` from the occurrence list, as one value tree that json, yaml and bin encode.

---

## Use cases

- **Typo dataset building**: Build a corpus of real-world typos and their corrections from your project's history. This can train spell-checking tools or IDE plugins.
- **Code quality auditing**: Identify patterns of common misspellings in your codebase to add to a linting dictionary.
- **API consistency**: Detect identifier typos that may cause confusion (e.g., `getUserByNmae` vs `getUserByName`).
- **Automated fix suggestions**: Use the typo dataset to build automated correction rules for CI pipelines.
- **Research**: Academic research on developer typo patterns and their prevalence across different languages.

---

## Limitations

- **UAST required**: Only languages with UAST parser support are analyzed. Identifiers in unsupported languages are not extracted.
- **Single-identifier changes only**: The analyzer only records a typo when exactly one identifier differs between the old and new lines after the shared identifiers cancel. Multi-identifier changes are skipped to avoid false positives.
- **Equal-length hunks only**: Only delete/insert hunk pairs with the same number of lines are considered. A typo fix that also adds or removes lines will be missed.
- **False positives**: Intentional identifier renames with small Levenshtein distance (e.g., `idx` to `jdx`) will be reported as typos.
- **One list entry per pair**: `typo_list` reports each `wrong → correct` pair once, at its first fix; the number of times the pair was fixed is in `patterns` (when above one) and in the per-file and aggregate counts.
- **CPU intensive**: Like all UAST-based analyzers, the typos analyzer parses both file versions for every changed file in every commit.

---

## See also

- [Typos reference](../analyzers/typos.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

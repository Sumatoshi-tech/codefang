# Understanding file and developer coupling

This page explains the mental model behind the couples analyzer: what file and
developer coupling mean, how coupling strength is computed, and how the walk
counts renames, merges and large commits. For the output schema, see the
[Couples reference](../analyzers/couples.md).

---

## What it measures

The couples analyzer identifies **file coupling** and **developer coupling** by
analyzing co-change patterns across Git history. Files that frequently change
together are likely coupled; developers who frequently touch the same files are
collaborating (or competing).

### File coupling

A square matrix where each cell `[i][j]` is the number of commits in the
window in which files `i` and `j` both changed. The matrix is symmetric and
the diagonal `[i][i]` is file `i`'s own change count. High co-change counts
indicate tight coupling between files.

**Coupling strength** is computed using the code-maat formula:

```text
strength = co_changes / average(changes_file1, changes_file2)
```

Where `changes_fileN` is the diagonal element of the file matrix. The result
is capped at 1.0. This normalizes coupling by the average activity of both
files, so a pair of files that change together 10 times out of 20 average
revisions scores 50%.

!!! info "Changeset size filter"
    Commits touching more than **1000 files** contribute no coupling
    context. Mass changes (formatting, license updates, dependency bumps)
    produce noise rather than meaningful coupling signal. Such a commit still
    counts for ownership: its author owns every file it touched, and its
    ndjson record keeps the commit's hash with an empty `CouplingFiles`
    list and the full `AuthorFiles` map.

### Developer coupling

A matrix where each cell `[i][j]` sums, over every file both developers
changed, `min(changes_i, changes_j)` — the number of file changes the two
developers share. The diagonal is a developer's own file-change count, and
the coupling strength uses the same code-maat formula over those counts.

### File ownership

For each file present at the window head, the analyzer reports its line
count (newlines in the blob at the head) and the number of distinct
contributors who changed it in the window. Single-owner files are flagged as
bus-factor risks.

### Rename tracking

The tree diff detects renames (a pure `git mv` and a move with edits above
the similarity threshold). When a rename is walked, the old path's whole
accumulated history — its co-change row and column, its change count and its
owners — moves to the new path, so the report shows ONE row under the new
name. A rename that also edits the file counts that commit as a change of the
new path. Renames chain (`a → b → c` ends up under `c`).

### Every change counts once

The walk visits commits oldest-first and keeps one set of files already
changed in the window. A merge commit (when `--first-parent` is off) lists,
against its first parent, every file the merged branch changed; those changes
were already counted at the branch's own commits, so the merge contributes
only the files no earlier commit of the window changed, and it records no
deletions. Under `--first-parent` the branch is not walked and the merge is
an ordinary commit that changes those files.

---

## Use cases

- **Architecture analysis**: Highly coupled file pairs that span package boundaries may indicate architectural violations.
- **Dependency discovery**: Find implicit dependencies that are not captured by import statements. If two files always change together, they are coupled even without explicit imports.
- **Module boundary validation**: Files within the same module should be more coupled than files across modules. Cross-module coupling is a design smell.
- **Team topology**: Developer coupling reveals who collaborates with whom. This can inform team structure decisions.
- **Change impact prediction**: When modifying a file, the coupling matrix predicts which other files are likely to need changes.
- **Bus factor assessment**: File ownership data identifies single-owner files that represent knowledge concentration risks.

---

## Architecture

The analyzer runs on the shared history core (one commit window, one
identity detector, one tick assigner, one change filter) in three steps:

1. **Per-commit products** (parallel, pure): the filtered tree diff of every
   commit against its first parent.

2. **Chronological reduce**: every commit, oldest-first, yields a payload —
   the coupling context (co-changed files, empty above the cap), the
   author's file touches and the rename pairs — which the in-memory
   aggregator sums into the file co-occurrence matrix and the per-developer
   file touches, applying each rename as it happens.

3. **Report**: the accumulated state is reduced to the files present in the
   newest commit of the window (with their line counts), and the typed
   metrics (`file_coupling`, `developer_coupling`, `file_ownership`,
   `aggregate`) are rendered for every format (text, plot, JSON, YAML,
   binary, ndjson, timeseries).

The output is a pure function of the repository and the flags: it does not
depend on the host CPU count.

---

## Methodology

The coupling strength formula follows the **code-maat** academic standard (Adam Tornhill, "Your Code as a Crime Scene"):

```text
degree = shared_revisions / average(revisions_A, revisions_B)
```

This measures what fraction of a file pair's average activity is shared. A coupling strength of 80% means the pair changes together in 80% of their average revision count.

The aggregate `avg_coupling_strength` is the mean of all per-pair coupling strengths (not the mean of raw co-change counts).

**Highly coupled pairs** are those with 10 or more co-changes (raw count threshold).

---

## Limitations

- **Large commits excluded from coupling**: Commits touching more than 1000 files feed no coupling context (they still count for ownership).
- **No temporal decay**: All commits are weighted equally. A coupling from long ago counts the same as a recent one. Consider `--since` or `--limit` for recent coupling analysis.
- **Merge commits**: A conflict resolution made inside a merge commit touches a file an earlier commit already changed, so it is not counted as a separate change. A merge whose branch commits fall outside the window is the first commit of the window that changes those files, so they count at the merge.
- **Head-only output**: Only files present in the newest commit of the window appear; files deleted inside the window are dropped from the matrices and the ownership list, and a file recreated after a deletion keeps the deleted file's counts under the same path.

---

## See also

- [Couples reference](../analyzers/couples.md) — output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

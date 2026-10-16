# Understanding file history analysis

This page explains the mental model behind the file history analyzer: what
per-file lifecycle data it records, how hotspots, churn and the composition
are derived, and how rename detection maintains continuity. For the output
schema, see the [File history reference](../analyzers/file-history.md).

---

## What it measures

The file history analyzer tracks the **lifecycle of every file** through Git
history, recording which commits touched each file, which developers modified
it, and aggregating line statistics per contributor. It follows renames to
maintain continuity across file moves.

### Per-file commit history

For each file present at the window head, the analyzer records the list of
commits in the window that created, modified or deleted it (a deletion
followed by a re-creation starts a new list: git treats that as a new file).

### Per-file contributor breakdown

For each file, a map of developer IDs to line statistics (added, removed,
changed), attributed commit by commit to the commit's author. A run of
removed lines immediately followed by added lines counts as *changed*
lines. Merge commits contribute no line stats (their diff against the first
parent repeats the merged branch's changes).

### Code hotspots

Files with high commit counts are flagged as hotspots with risk levels:

!!! warning "Hotspot risk levels"
    - **CRITICAL**: >= 50 commits
    - **HIGH**: >= 30 commits
    - **MEDIUM**: >= 15 commits
    - Files below 15 commits are not flagged

### File churn

A composite score combining commit frequency and line change volume:
`commit_count + (added + removed + changed) / 100`. High-churn files may
indicate instability or areas of active development.

### File composition

Every changed file is classified by the shared classifier over its path
**and** its blob content (the first 8 KiB): `binary` (NUL bytes), `image`,
`vendor`, `generated` (generated paths or a generated-code marker in the
content), `documentation`, `configuration`, `dotfile`, else `source`.

- `composition` counts the **distinct files changed in the window** per
  category: a file edited in eight commits is one file. A single-file
  repository therefore reports `source 1 (100%)`, not the number of edits.
- `composition_ts` counts the distinct files changed in **each tick**
  (`--tick-size`, 24 h by default), with the tick's `start_time` /
  `end_time` — the first and last committer time of the tick's commits.

A file's latest classification wins (text that became binary counts as
binary), and a rename retires the old path, so the moved file is counted
once. The composition covers every file changed in the window, including
files deleted before the window head; the per-file sections cover only the
files at the head.

### Rename support

When Git detects a file rename (a pure `git mv`, or a move with edits above
the similarity threshold), the analyzer transfers the full commit list from
the old path to the new path and appends the rename commit, maintaining a
continuous record; the rename's own line stats (the edit, if any) are
attributed to the new path. The ndjson record of the commit carries the
`FromPath` / `ToPath` pair.

---

## Use cases

- **Hotspot identification**: Find the files that change most often. These are the highest-risk files for bugs and the best candidates for extra test coverage and code review scrutiny.
- **Code ownership mapping**: Determine who is the primary contributor for each file to establish code ownership.
- **Onboarding guides**: New team members can see which files each developer owns to know who to ask about specific code.
- **Refactoring ROI**: Identify files with both high churn and many contributors -- refactoring these produces the largest payoff.
- **Risk assessment**: Files with a single contributor and high commit counts are both hot and concentrated -- a bus factor risk.
- **Change mix over time**: The composition time series shows whether a period's changes went into source, documentation, configuration or generated files.

---

## Limitations

- **HEAD-only per-file output**: Only files present in the newest commit of the window are listed in `file_churn`, `file_contributors`, `hotspots` and the aggregate. Files deleted inside the window are tracked during analysis (and counted in the composition) but excluded from the per-file results.
- **Merge handling**: A merge commit appends to the commit lists of the files it lists against its first parent and counts them in its tick's composition, but contributes no line stats. Under `--first-parent` a merge is an ordinary commit.
- **Rename detection**: Depends on Git's rename detection heuristics (50 % similarity). A file that is renamed and rewritten beyond that threshold in one commit is a deletion plus an insertion.
- **Churn score formula**: The churn score is `commit_count + (added + removed + changed) / 100`. This is a simple heuristic, not a rigorous statistical measure.
- **Binary blobs**: A change whose blob is binary contributes no line stats (it still counts as a commit and as a `binary` file in the composition).

---

## See also

- [File history reference](../analyzers/file-history.md) — output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

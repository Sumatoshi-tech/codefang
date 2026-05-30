# Understanding file history analysis

This page explains the mental model behind the file history analyzer: what
per-file lifecycle data it records, how hotspots and churn are derived, and how
rename detection maintains continuity. For configuration keys and the output
schema, see the [File history reference](../analyzers/file-history.md).

---

## What it measures

The file history analyzer tracks the **lifecycle of every file** through Git
history, recording which commits touched each file, which developers modified
it, and aggregating line statistics per contributor. It supports rename
detection to maintain continuity across file moves.

### Per-file commit history

For each file present in the repository at HEAD, the analyzer records the ordered list of commits that created, modified, or deleted it.

### Per-file contributor breakdown

For each file, a map of developer IDs to line statistics (added, removed, changed). This shows who contributed what to each file.

### Code hotspots

Files with high commit counts are flagged as hotspots with risk levels:

!!! warning "Hotspot risk levels"
    - **CRITICAL**: >= 50 commits
    - **HIGH**: >= 30 commits
    - **MEDIUM**: >= 15 commits
    - Files below 15 commits are not flagged

### File churn

A composite score combining commit frequency and line change volume. High-churn files may indicate instability or areas of active development.

### Rename support

When Git detects a file rename (e.g., `old/path.go` to `new/path.go`), the analyzer transfers the full history from the old path to the new path, maintaining a continuous record.

---

## Use cases

- **Hotspot identification**: Find the files that change most often. These are the highest-risk files for bugs and the best candidates for extra test coverage and code review scrutiny.
- **Code ownership mapping**: Determine who is the primary contributor for each file to establish code ownership.
- **Onboarding guides**: New team members can see which files each developer owns to know who to ask about specific code.
- **Refactoring ROI**: Identify files with both high churn and many contributors -- refactoring these produces the largest payoff.
- **Risk assessment**: Files with a single contributor and high commit counts are both hot and concentrated -- a bus factor risk.

---

## Limitations

- **HEAD-only output**: Only files present at HEAD are included in the final output. Files that were deleted before the last commit are tracked during analysis but excluded from results.
- **Merge handling**: Merge commits are processed only once to avoid double-counting. Non-merge context changes in merge commits are skipped.
- **Rename detection**: Depends on Git's rename detection heuristics. If a file is both renamed and heavily modified in the same commit, Git may not detect the rename.
- **Churn score formula**: The churn score is `commit_count + (added + removed + changed) / 100`. This is a simple heuristic, not a rigorous statistical measure.
- **No time-series**: Unlike the burndown analyzer, file history does not produce time-series data. It provides aggregate per-file statistics only.

---

## See also

- [File history reference](../analyzers/file-history.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run history analysis.

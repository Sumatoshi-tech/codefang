# File History Analysis

## Preface
Files are the primary unit of organization in most codebases. Tracking their history provides a granular view of the project's evolution.

## Problem
It's often necessary to trace the complete lifecycle of a file or a set of files:
- "When was this file created?"
- "Who has touched this specific file over its lifetime?"
- "Which commits modified this file?"

## How analyzer solves it
The File History analyzer creates a detailed map for every file in the repository. It lists every commit that modified the file and aggregates the line statistics (added/removed/changed) for each developer who touched it.

## Historical context
This is a foundational analysis that powers more complex views. It's essentially an aggregated "git log --stat" for every file.

## Real world examples
- **Audit Trails:** Tracing the history of sensitive configuration files.
- **Hotspot Analysis:** Identifying files that are changed most frequently.

## How analyzer works here
1.  **Change Tracking:** Listens to TreeDiff events to detect file creations, modifications, and deletions.
2.  **Rename Handling:** Robustly tracks files across renames so history isn't lost.
3.  **Aggregation:** Stores a list of commit hashes and a map of Developer -> LineStats for each file.

## Limitations
- **Volume:** For very large repositories with millions of files, this can produce a massive amount of data.

## Further plans
- Visualization of file lifecycles.

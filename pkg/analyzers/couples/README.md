# Couples Analysis

## Preface
In a software system, components that change together, stay together. Logical coupling often differs from static dependency coupling.

## Problem
Sometimes two files are logically coupled (e.g., a frontend view and a backend API handler) but have no direct static reference. When one changes, the other must change. If a developer forgets this, bugs occur.

## How analyzer solves it
The Couples analyzer looks at the commit history to find files that are frequently modified in the same commit ("co-changed"). It also looks at developers who work on the same files.

## Historical context
Logical Coupling (or Evolutionary Coupling) analysis has been a research topic since the late 90s. It reveals hidden dependencies that static analysis misses.

## Real world examples
- **Hidden Dependencies:** Discovering that changing `Config.java` almost always requires changing `DeployScript.sh`.
- **Team Coordination:** Identifying developers who should coordinate because they frequently touch the same code areas.

## How analyzer works here
1.  **Commit Analysis:** For each commit, it lists the set of changed files.
2.  **Co-occurrence Matrix:** It builds a matrix where `matrix[fileA][fileB]` counts how many commits included both `fileA` and `fileB`.
3.  **Developer Coupling:** Similarly, it tracks which developers edit the same files, building a developer-developer interaction matrix.

## Limitations
- **Commit Granularity:** It assumes that atomic commits represent logical units of work. "Squash commits" or bad commit discipline can skew results.
- **Matrix Size:** The file matrix can get very large ($N^2$) for large repositories.

## Further plans
- Temporal coupling: detecting files changed within a short time window but not necessarily in the same commit.

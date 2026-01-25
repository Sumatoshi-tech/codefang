# Developers Analysis

## Preface
Software is built by people. Understanding the activity and contributions of the development team is essential for project management, though it must be done with care to avoid misuse.

## Problem
Managers and teams often lack visibility into the distribution of work:
- "Who is the primary contributor to this project?"
- "Is the workload distributed evenly?"
- "Which languages is each developer working with?"

## How analyzer solves it
The Devs analyzer computes high-level activity statistics for each developer. It tracks:
- Number of commits.
- Lines added, removed, and changed.
- Breakdown of activity by programming language.

## Historical context
Counting lines of code (LOC) and commits is one of the oldest forms of software metrics. While widely criticized as a measure of *productivity* (quality != quantity), it remains a valid measure of *activity* and *impact*.

## Real world examples
- **Bus Factor:** Identifying key developers whose absence would stall the project.
- **Language Expertise:** Finding out who on the team is writing the most Go vs. Python code.

## How analyzer works here
1.  **Identity Merging:** Uses the `IdentityDetector` to merge multiple emails/names for the same person (using `.mailmap` or heuristics).
2.  **Diff Analysis:** For every commit, it calculates the line stats (added/removed/changed).
3.  **Aggregation:** Aggregates these stats per author and per time interval (tick).
4.  **Language Detection:** Maps files to languages to provide a language-specific breakdown.

## Limitations
- **LOC is not Productivity:** This analyzer does not measure code quality or problem-solving value. A deletion of 1000 lines can be more valuable than an addition of 1000 lines.
- **Squashed Commits:** Squashing commits can obscure individual contributions.

## Further plans
- More nuanced metrics (e.g., "churn" vs. "productive code").

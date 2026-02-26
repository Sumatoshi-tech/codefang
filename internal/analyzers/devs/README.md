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

## Metrics

The devs analyzer computes the following metrics. Each metric implements the `Metric[In, Out]` interface from `pkg/metrics`, making them reusable across output formats (plot, JSON, YAML).

### developers
**Type:** `list`

Per-developer contribution statistics including commits, lines added/removed, language breakdown, and activity timeline. Developers are sorted by commit count.

**Output fields:**
- `id` - Developer identifier
- `name` - Developer name (from identity resolution)
- `commits` - Total number of commits
- `lines_added` - Total lines added
- `lines_removed` - Total lines removed
- `lines_changed` - Total lines changed (modifications)
- `net_lines` - Net line change (added - removed)
- `languages` - Map of language to line stats
- `first_tick` - First active time period
- `last_tick` - Last active time period
- `active_ticks` - Number of periods with activity

### languages
**Type:** `list`

Per-language contribution statistics showing total lines and contributor breakdown. Languages are sorted by total lines added.

**Output fields:**
- `name` - Language name
- `total_lines` - Total lines added in this language
- `contributors` - Map of developer ID to lines added

### bus_factor
**Type:** `risk`

Knowledge concentration risk per language. Measures how dependent each language's codebase is on individual contributors.

**Risk levels:**
- `CRITICAL` - Single developer owns >= 90% of codebase
- `HIGH` - Single developer owns >= 80%
- `MEDIUM` - Single developer owns >= 60%
- `LOW` - Single developer owns < 60%

**Output fields:**
- `language` - Language name
- `primary_dev_id` - ID of top contributor
- `primary_dev_name` - Name of top contributor
- `primary_percentage` - Percentage owned by primary
- `secondary_dev_id` - ID of second contributor (if exists)
- `secondary_dev_name` - Name of second contributor
- `secondary_percentage` - Percentage owned by secondary
- `risk_level` - Risk classification

### activity
**Type:** `time_series`

Time-series of commit activity per tick, broken down by developer. Shows contribution velocity over the analysis period.

**Output fields:**
- `tick` - Time period index
- `by_developer` - Map of developer ID to commit count
- `total_commits` - Total commits in this tick

### churn
**Type:** `time_series`

Time-series of lines added and removed per tick. High churn may indicate refactoring, feature development, or instability.

**Output fields:**
- `tick` - Time period index
- `lines_added` - Lines added in this tick
- `lines_removed` - Lines removed in this tick
- `net_change` - Net line change (added - removed)

### aggregate
**Type:** `aggregate`

Aggregate statistics across all developers and the analysis period.

**Output fields:**
- `total_commits` - Total commits across all developers
- `total_lines_added` - Total lines added
- `total_lines_removed` - Total lines removed
- `total_developers` - Number of unique developers
- `active_developers` - Developers with commits in recent 30% of period
- `analysis_period_ticks` - Number of time periods analyzed

## Further plans
- More nuanced metrics (e.g., "churn" vs. "productive code").

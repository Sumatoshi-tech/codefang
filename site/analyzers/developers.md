# Developers Analyzer

The developers analyzer computes **per-developer contribution statistics** across Git history, including commit counts, line changes, language breakdown, bus factor risk, and activity time series.

---

## Quick Start

```bash
codefang run -a history/devs .
```

With anonymization:

```bash
codefang run -a history/devs --anonymize .
```

Text output (terminal-friendly):

```bash
codefang run -a history/devs -f text .
```

---

## Architecture

The developers analyzer is built on the **BaseHistoryAnalyzer** and **GenericAggregator** foundations:

1. **Consume phase**: `Consume()` extracts author ID, line stats, and language breakdown, delegating state storage to the generic aggregator framework. The analyzer retains minimal internal state.
2. **Aggregation phase**: The `GenericAggregator` automatically handles per-commit memory spilling and per-tick grouping using pure function hooks (`extractTC`, `mergeState`, `buildTick`), eliminating custom state management boilerplate.
3. **Metrics & UI phase**: A pure function pipeline (`ComputeAllMetrics`) generates the output structures for JSON, YAML, and text, while declarative chart builders (`plotpage.BuildBarChart`, `plotpage.BuildLineChart`) render the HTML visualizations.

This unified approach significantly reduces boilerplate while maintaining full support for streaming output, decoupled aggregation, and budget-aware memory spilling.

---

## What It Measures

### Developer Statistics

For each developer (identified by the identity detector):

- **Commits**: Total number of commits
- **Lines added / removed / changed**: Aggregate line statistics
- **Net lines**: `added - removed`, showing net contribution
- **Languages**: Breakdown of line changes per programming language
- **Active period**: First and last ticks of activity, number of active ticks

### Language Statistics

Aggregated across all developers:

- **Total lines per language**: Lines added (for backward compatibility)
- **Total contribution per language**: Lines added + removed, used for bus factor and ownership calculations
- **Contributors per language**: Which developers contribute to each language, measured by total contribution (added + removed)

### Bus Factor

Knowledge concentration risk per language, following the [CHAOSS Contributor Absence Factor](https://chaoss.community/kb/metric-bus-factor/) methodology.

For each language, the analyzer computes:

- **Bus factor number**: The smallest number of contributors responsible for 50% of total contributions (added + removed). This is the CHAOSS standard metric.
- **Total contributors**: Number of unique contributors to that language.
- **Primary/secondary owner**: The top two contributors and their ownership percentages.
- **Risk level**: Based on primary owner concentration.

!!! danger "Risk levels"
    - **CRITICAL** (>= 90%): A single developer owns nearly all the code
    - **HIGH** (>= 80%): Very concentrated ownership
    - **MEDIUM** (>= 60%): Moderate concentration
    - **LOW** (< 60%): Healthy distribution

A **project-level bus factor** is also computed across all developers and all languages, reported in the aggregate section.

### Activity Time Series

Per-tick commit counts broken down by developer. Shows contribution velocity over time.

### Code Churn (Line Velocity)

Per-tick lines added and removed. This measures raw line velocity — the volume of code changes over time. High values may indicate refactoring, feature development, or instability.

!!! note "Terminology"
    In academic literature, "code churn" specifically refers to recently-written code that is quickly rewritten. This analyzer measures the broader concept of line velocity (total additions and removals per time period).

---

## Configuration Options

| Option | Type | Default | Description |
|---|---|---|---|
| `Devs.ConsiderEmptyCommits` | `bool` | `false` | Include empty commits (e.g., trivial merges) in commit counts |
| `Devs.Anonymize` | `bool` | `false` | Replace developer names with pseudonyms (Developer-A, Developer-B, etc.) |

```yaml
# .codefang.yml
history:
  devs:
    consider_empty_commits: false
    anonymize: false
```

---

## Example Output

=== "JSON"

    ```json
    {
      "developers": [
        {
          "id": 0,
          "name": "alice",
          "commits": 342,
          "lines_added": 28500,
          "lines_removed": 12300,
          "lines_changed": 8400,
          "net_lines": 16200,
          "languages": {
            "Go": {"added": 22000, "removed": 9800, "changed": 6200},
            "Python": {"added": 6500, "removed": 2500, "changed": 2200}
          },
          "first_tick": 0,
          "last_tick": 120,
          "active_ticks": 85
        }
      ],
      "languages": [
        {
          "name": "Go",
          "total_lines": 45000,
          "total_contribution": 67800,
          "contributors": {"0": 54600, "1": 13200}
        },
        {
          "name": "Python",
          "total_lines": 12000,
          "total_contribution": 16700,
          "contributors": {"0": 11200, "1": 5500}
        }
      ],
      "busfactor": [
        {
          "language": "Python",
          "bus_factor": 1,
          "total_contributors": 2,
          "primary_dev_name": "alice",
          "primary_percentage": 67.1,
          "secondary_dev_name": "bob",
          "secondary_percentage": 32.9,
          "risk_level": "MEDIUM"
        }
      ],
      "activity": [
        {"tick": 0, "total_commits": 5, "by_developer": {"0": 3, "1": 2}},
        {"tick": 1, "total_commits": 8, "by_developer": {"0": 5, "1": 3}}
      ],
      "churn": [
        {"tick": 0, "lines_added": 450, "lines_removed": 120, "net_change": 330}
      ],
      "aggregate": {
        "total_commits": 850,
        "total_lines_added": 95000,
        "total_lines_removed": 42000,
        "total_developers": 5,
        "active_developers": 3,
        "analysis_period_ticks": 120,
        "project_bus_factor": 2,
        "total_languages": 4
      }
    }
    ```

=== "YAML"

    ```yaml
    aggregate:
      total_commits: 850
      total_lines_added: 95000
      total_lines_removed: 42000
      total_developers: 5
      active_developers: 3
      project_bus_factor: 2
      total_languages: 4
    developers:
      - name: alice
        commits: 342
        lines_added: 28500
        lines_removed: 12300
        net_lines: 16200
    languages:
      - name: Go
        total_lines: 45000
        total_contribution: 67800
    busfactor:
      - language: Python
        bus_factor: 1
        total_contributors: 2
        primary_dev_name: alice
        primary_percentage: 67.1
        risk_level: MEDIUM
    ```

---

## Use Cases

- **Team assessment**: Understand who contributes what, in which languages, and when.
- **Bus factor analysis**: Identify languages or components where a single developer departure would create critical knowledge gaps. The CHAOSS bus factor number tells you how many people need to leave before 50% of knowledge is lost.
- **Activity monitoring**: Track developer engagement over time. Declining activity may signal burnout or attrition risk.
- **Language migration tracking**: Monitor the adoption of a new language by watching language statistics over time.
- **Onboarding evaluation**: Measure how quickly new team members ramp up by comparing their activity curves.
- **Code churn analysis**: Detect periods of high line velocity that may correlate with instability or deadline pressure.

---

## Limitations

- **Identity resolution**: Developer identity is determined by the identity detector (email-based by default). Multiple email addresses for the same person will appear as separate developers unless a mailmap is configured.
- **Merge commits**: By default, merge commits are processed only once (first encounter). Trivial merges are skipped unless `ConsiderEmptyCommits` is enabled.
- **Line attribution**: Lines are attributed to the commit author, not the committer. In workflows with heavy rebasing, this may differ from expectations.
- **Contribution measurement**: Contributions are measured as lines added + lines removed. This gives fair credit to refactoring work but does not distinguish between code complexity or quality.
- **Active developer threshold**: "Active developers" are those with commits in the last 90 days (when tick size is known). Falls back to the recent 30% of the analysis period when tick size is unavailable. This threshold is not configurable.

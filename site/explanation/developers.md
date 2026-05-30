# Understanding developer analysis

This page explains the mental model behind the developers analyzer: what
per-developer statistics it computes, how the bus factor is derived, and how
the aggregation pipeline is structured. For configuration keys and the output
schema, see the [Developers reference](../analyzers/developers.md).

---

## What it measures

The developers analyzer computes **per-developer contribution statistics**
across Git history, including commit counts, line changes, language breakdown,
bus factor risk, and activity time series.

### Developer statistics

For each developer (identified by the identity detector):

- **Commits**: Total number of commits
- **Lines added / removed / changed**: Aggregate line statistics
- **Net lines**: `added - removed`, showing net contribution
- **Languages**: Breakdown of line changes per programming language
- **Active period**: First and last ticks of activity, number of active ticks

### Language statistics

Aggregated across all developers:

- **Total lines per language**: Lines added (for backward compatibility)
- **Total contribution per language**: Lines added + removed, used for bus factor and ownership calculations
- **Contributors per language**: Which developers contribute to each language, measured by total contribution (added + removed)

### Bus factor

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

### Activity time series

Per-tick commit counts broken down by developer. Shows contribution velocity over time.

### Code churn (line velocity)

Per-tick lines added and removed. This measures raw line velocity — the volume of code changes over time. High values may indicate refactoring, feature development, or instability.

!!! note "Terminology"
    In academic literature, "code churn" specifically refers to recently-written code that is quickly rewritten. This analyzer measures the broader concept of line velocity (total additions and removals per time period).

---

## Architecture

The developers analyzer is built on the **BaseHistoryAnalyzer** and **GenericAggregator** foundations:

1. **Consume phase**: `Consume()` extracts author ID, line stats, and language breakdown, delegating state storage to the generic aggregator framework. The analyzer retains minimal internal state.
2. **Aggregation phase**: The `GenericAggregator` automatically handles per-commit memory spilling and per-tick grouping using pure function hooks (`extractTC`, `mergeState`, `buildTick`), eliminating custom state management boilerplate.
3. **Metrics & UI phase**: A pure function pipeline (`ComputeAllMetrics`) generates the output structures for JSON, YAML, and text, while declarative chart builders (`plotpage.BuildBarChart`, `plotpage.BuildLineChart`) render the HTML visualizations.

This unified approach significantly reduces boilerplate while maintaining full support for streaming output, decoupled aggregation, and budget-aware memory spilling.

---

## Use cases

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
- **Active developer threshold**: "Active developers" are those with commits in the most recent 90-day window (when tick size is known). Falls back to the recent 30% of the analysis period when tick size is unavailable. This threshold is not configurable.

---

## See also

- [Developers reference](../analyzers/developers.md) — configuration keys and output schema.
- [Quick start](../getting-started/quickstart.md) — run developer analysis.

# Understanding developer analysis

This page explains the mental model behind the developers analyzer: what
per-developer statistics it computes, how the bus factor is derived, and how
the aggregation pipeline is structured. For the flags and the output
schema, see the [Developers reference](../analyzers/developers.md).

---

## What it measures

The developers analyzer computes **per-developer contribution statistics**
across Git history: commit counts, line changes, language breakdown, bus
factor risk and activity time series.

### Developer statistics

For each developer (identified by the shared identity detector, so
`--people-dict`, `--exact-signatures` and `--anonymize` apply):

- **Commits**: the number of commits attributed to the developer.
- **Lines added / removed / changed**: per commit, every file change is
  line-diffed against the first parent. A deleted run immediately followed
  by an inserted run counts as *changed* lines (the shorter of the two), the
  rest as added or removed. A new file adds its line count, a deleted file
  removes it, an empty (0-byte) blob is a 0-line text side, and a pure
  rename is a 0 / 0 / 0 change that still registers the file's language.
  Binary blobs carry no line statistics.
- **Net lines**: `added − removed`.
- **Languages**: the same statistics per language, detected from the path
  and the blob content (`Other` when no language matches).
- **Active period**: first and last ticks of activity, number of distinct
  active ticks.

### Which commits count

The devs walk visits every commit of the window in the user's walk mode
(`--first-parent` or the full DAG, unlike burndown which always follows
the first parent). A merge commit counts as a commit for its author but
contributes no line statistics — its lines belong to the commits it merges.
A commit whose filtered tree diff is empty (a trivial merge, an
`--allow-empty` commit, a commit touching only excluded paths) is skipped
unless `--empty-commits` is set, in which case it counts as a commit with
zero lines. Commits whose raw tree diff exceeds `--max-changes-per-commit`
are dropped before any analyzer sees them.

### Language statistics

Aggregated across all developers:

- **Total lines per language**: lines added.
- **Total contribution per language**: lines added + removed, the measure
  the bus factor and the ownership shares use.
- **Contributors per language**: each developer's contribution.

### Bus factor

Knowledge-concentration risk per language, following the [CHAOSS
Contributor Absence Factor](https://chaoss.community/kb/metric-bus-factor/)
methodology. For each language:

- **Bus factor number**: the smallest number of contributors responsible
  for 50 % of the total contribution.
- **Total contributors**: unique contributors to that language.
- **Primary / secondary owner**: the top two contributors with their
  shares. The secondary owner is reported whenever the language has at
  least two contributors — developer id 0 included.
- **Risk level**: from the primary owner's share.

!!! danger "Risk levels"
    - **CRITICAL** (>= 90%): a single developer owns nearly all the code
    - **HIGH** (>= 80%): very concentrated ownership
    - **MEDIUM** (>= 60%): moderate concentration
    - **LOW** (< 60%): healthy distribution

A **project-level bus factor** over all developers and languages is
reported in the aggregate section.

### Activity time series

Per-tick commit counts broken down by developer, with the tick's first and
last committer time.

### Code churn (line velocity)

Per-tick lines added and removed. High values may indicate refactoring,
feature development or instability.

!!! note "Terminology"
    In academic literature, "code churn" specifically refers to
    recently-written code that is quickly rewritten. This analyzer measures
    the broader concept of line velocity (total additions and removals per
    time period).

### Aggregate

`total_developers` and `active_developers` are exact counts;
`estimated_total_developers` and `estimated_active_developers` are the
same cardinalities estimated by a HyperLogLog sketch (precision 14, about
0.8 % standard error) — they are the figures a streaming or merged run can
maintain without the exact sets, and they equal the exact counts on small
inputs. `analysis_period_ticks` is the newest tick of the window.

---

## Architecture

1. **Per-commit pre-pass** (parallel): the tree diff against the first
   parent, the cap check, the change filter, the per-change line
   statistics and the per-blob language.
2. **Chronological reduce**: the shared identity / tick stamps key every
   commit's `CommitDevData`, grouped into per-tick / per-developer buckets.
3. **Metrics**: a pure function pipeline (`compute_all_metrics`) produces
   the developers, languages, bus factor, activity, churn and aggregate
   sections; the json / yaml / bin / text / plot formats are encodings of
   that one value, and the timeseries formats list every kept commit in
   tick order.

The output is a pure function of the repository and the flags: two runs
produce identical bytes.

---

## Use cases

- **Team assessment**: who contributes what, in which languages, and when.
- **Bus factor analysis**: languages where a single departure would create
  a critical knowledge gap.
- **Activity monitoring**: declining activity may signal attrition risk.
- **Language migration tracking**: watch language statistics over time.
- **Onboarding evaluation**: compare the activity curves of new team
  members.
- **Code churn analysis**: periods of high line velocity.

---

## Limitations

- **Identity resolution**: identities are merged by the shared detector
  (a name or an e-mail seen together links the two). Use `--people-dict`
  for explicit aliases and `--exact-signatures` to keep every
  `name <email>` pair apart.
- **Merge commits**: counted as commits, never as lines.
- **Line attribution**: lines are attributed to the commit author, not the
  committer.
- **Contribution measurement**: `added + removed` credits refactoring work
  but does not distinguish complexity or quality.
- **Active developer threshold**: "active developers" are those with
  commits in the most recent 90 days (`90 · 24 / tick_size_hours` ticks,
  clamped at the start of the window). The threshold is not configurable.

---

## See also

- [Developers reference](../analyzers/developers.md) — flags and output
  schema.
- [Quick start](../getting-started/quickstart.md) — run developer analysis.

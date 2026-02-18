# Temporal Anomaly Detection

## Preface
Large codebases evolve through thousands of commits. Most commits follow a predictable rhythm — a handful of files changed, tens to hundreds of lines modified. When a commit or time period suddenly deviates from that baseline, it often signals something worth investigating.

## Problem
Teams need to detect sudden quality degradation in commit history:
- "When did the massive vendoring event happen that doubled our codebase?"
- "Which time periods had abnormal churn that might indicate rushed changes?"
- "Are there refactoring bursts that correlate with post-release cleanup?"

Manual review of thousands of commits is impractical. Simple thresholds (e.g., ">100 files changed") miss context: 100 files might be normal for a monorepo but alarming for a small service.

## How analyzer solves it
The Anomaly analyzer applies Z-score statistical analysis over a sliding window of per-tick metrics. Instead of fixed thresholds, it detects deviations relative to the repository's own recent baseline. A commit changing 500 files is flagged only if the surrounding ticks average 20 files — it adapts to each repository's rhythm.

## Historical context
Z-score anomaly detection is a foundational technique in statistical process control (SPC), originating from Walter Shewhart's control charts at Bell Labs in the 1920s. The concept is simple: if a data point is more than N standard deviations from the mean, it is an outlier. Applied to software engineering, this detects "change-point events" in repository evolution — moments where development patterns shift abruptly.

## Real world examples
- **Vendor imports:** A sudden spike of thousands of added lines with zero removed lines flags a bulk import (e.g., vendoring a dependency).
- **Major refactors:** A tick with 500+ files changed when the rolling average is 30 files indicates a large-scale restructuring.
- **Release cleanup:** Post-release ticks often show abnormal churn as teams fix accumulated technical debt.
- **Regressions:** A sudden drop in net churn (large deletions) after a period of steady growth may indicate a reverted feature.

## How analyzer works here
1. **Metric collection:** For each commit, the analyzer records per-tick metrics from plumbing analyzers: files changed, lines added, lines removed, and net churn (added - removed).
2. **Tick aggregation:** Multiple commits in the same time tick are aggregated into a single data point.
3. **Z-score computation:** For each tick, a trailing sliding window (default: 20 ticks) computes the rolling mean and population standard deviation. The Z-score measures how many standard deviations the current tick deviates from the window.
4. **Multi-metric detection:** Z-scores are computed independently for all four metrics. A tick is flagged as anomalous if any metric exceeds the threshold (default: 2.0 sigma).
5. **Severity ranking:** Anomalies are sorted by the maximum absolute Z-score across all metrics, so the most extreme deviations appear first.

### Zero-variance handling
When the sliding window has zero variance (all identical values) and the current value differs, a sentinel Z-score of 100.0 is assigned. This correctly flags a spike against a perfectly stable baseline (e.g., the first non-trivial commit after a series of identical ticks).

## Limitations
- **Tick granularity:** The analyzer operates on ticks (time buckets), not individual commits. Anomalies point to ticks, not specific commit hashes.
- **No semantic analysis:** A 1000-line addition of auto-generated code and a 1000-line hand-written refactor look identical. The analyzer detects *statistical* anomalies, not *semantic* ones.
- **Early window noise:** The first few ticks have small sliding windows, making Z-scores less reliable. The sentinel value (100.0) handles the extreme case but borderline anomalies in early ticks should be interpreted with caution.
- **Merge commits:** With `--first-parent`, merge commits are treated as single events. Without it, individual commits within a merge are counted separately, which may dilute or amplify anomalies.

## Configuration

| Key | CLI Flag | Default | Description |
|-----|----------|---------|-------------|
| `history.anomaly.threshold` | `--anomaly-threshold` | `2.0` | Z-score threshold in standard deviations |
| `history.anomaly.window_size` | `--anomaly-window` | `20` | Sliding window size in ticks |

Lower threshold = more sensitive (more anomalies detected). Higher window = smoother baseline (less reactive to recent changes).

## Metrics

Each metric implements the `Metric[In, Out]` interface from `pkg/metrics`.

### anomalies
**Type:** `risk`

List of detected anomalous ticks, sorted by severity (highest absolute Z-score first).

**Output fields:**
- `tick` - Time period index where anomaly was detected
- `z_scores` - Per-metric Z-scores (`net_churn`, `files_changed`, `lines_added`, `lines_removed`)
- `max_abs_z_score` - Maximum absolute Z-score across all metrics (severity measure)
- `metrics` - Raw metric values for the tick (`files_changed`, `lines_added`, `lines_removed`, `net_churn`)
- `files` - List of files changed in the anomalous tick

### time_series
**Type:** `time_series`

Per-tick metrics with anomaly annotations. Every tick in the analysis period has an entry, regardless of whether it was flagged as anomalous.

**Output fields:**
- `tick` - Time period index
- `metrics` - Raw metric values (`files_changed`, `lines_added`, `lines_removed`, `net_churn`)
- `is_anomaly` - Whether the tick was flagged as anomalous
- `churn_z_score` - Net churn Z-score for the tick

### aggregate
**Type:** `aggregate`

Summary statistics for the entire analysis period.

**Output fields:**
- `total_ticks` - Number of time periods analyzed
- `total_anomalies` - Number of ticks flagged as anomalous
- `anomaly_rate` - Percentage of ticks that are anomalous
- `threshold` - Z-score threshold used
- `window_size` - Sliding window size used
- `churn_mean` - Mean net churn across all ticks
- `churn_stddev` - Standard deviation of net churn
- `files_mean` - Mean files changed per tick
- `files_stddev` - Standard deviation of files changed

## Plot output

The plot format (`--format plot`) generates an interactive HTML report with:
1. **Net Churn Over Time** - Line chart showing net churn per tick with anomalous ticks highlighted as red scatter points.
2. **Anomaly Detection Summary** - Stats grid showing total ticks, anomalies detected, anomaly rate (with color-coded badge), and highest Z-score.

## Further plans
- Per-file anomaly detection (flag files that appear disproportionately in anomalous ticks).
- Commit-level granularity option (detect anomalous individual commits, not just ticks).
- Correlation with other analyzers (e.g., complexity spikes in anomalous ticks).
- Configurable metric weights (e.g., prioritize files_changed over lines_added).

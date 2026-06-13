//! File history metric computation.
//!
//! Given the raw per-file history (commit-hash lists and per-author line
//! stats), this module derives:
//!
//! * [`FileChurnData`] — per-file churn, sorted by churn score descending;
//! * [`FileContributorData`] — per-file contributor breakdown;
//! * [`HotspotData`] — high-churn files at/above the medium risk threshold,
//!   sorted by risk then commit count;
//! * [`AggregateData`] — repository-wide summary statistics;
//! * [`CompositionData`] / [`CompositionTimeSeriesEntry`] — file category mix.
//!
//! Sort orders and arithmetic are part of the report contract (pinned by the
//! differential gate) so the derived numeric values are byte-identical after
//! rendering through [`crate::report`].

use std::collections::BTreeMap;

use cf_metrics::{risk_priority, RiskLevel};

use crate::classify::ALL_CATEGORIES;
use crate::tc::{CategoryCounts, LineStats};

/// Churn score divisor for normalization.
pub const CHURN_SCORE_DIVISOR: f64 = 100.0;
/// Percentage multiplier.
pub const PERCENT_MULTIPLIER: f64 = 100.0;

/// Hotspot risk threshold (commits) for critical risk.
pub const HOTSPOT_THRESHOLD_CRITICAL: i64 = 50;
/// Hotspot risk threshold (commits) for high risk.
pub const HOTSPOT_THRESHOLD_HIGH: i64 = 30;
/// Hotspot risk threshold (commits) for medium risk.
pub const HOTSPOT_THRESHOLD_MEDIUM: i64 = 15;

/// The change history for a single file: a map from developer id to line
/// stats, plus the list of commit hashes that touched the file.
///
/// Only the **length** of `hashes` (commit count) feeds the metrics, so the
/// hash order (nondeterministic in the reference implementation) does not
/// affect any derived numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileHistory {
    /// Developer id -> aggregated line stats for this file.
    pub people: BTreeMap<i64, LineStats>,
    /// Commit hashes that touched this file (string form).
    pub hashes: Vec<String>,
}

/// Parsed input for metric computation.
#[derive(Debug, Clone, Default)]
pub struct ReportData {
    /// File path -> history.
    pub files: BTreeMap<String, FileHistory>,
}

/// Churn statistics for a single file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileChurnData {
    /// File path.
    pub path: String,
    /// Number of commits that touched the file.
    pub commit_count: i64,
    /// Number of distinct contributors.
    pub contributor_count: i64,
    /// Total lines added across all contributors.
    pub total_added: i64,
    /// Total lines removed.
    pub total_removed: i64,
    /// Total lines changed.
    pub total_changed: i64,
    /// Composite churn score.
    pub churn_score: f64,
}

/// Line stats for a single contributor to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributorEntry {
    /// Developer id.
    pub dev_id: i64,
    /// Lines added.
    pub added: i64,
    /// Lines removed.
    pub removed: i64,
    /// Lines changed.
    pub changed: i64,
}

/// Contributor breakdown for a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContributorData {
    /// File path.
    pub path: String,
    /// Per-contributor stats, sorted ascending by developer id.
    pub contributors: Vec<ContributorEntry>,
    /// Developer id with the most (added + changed) lines.
    pub top_contributor_id: i64,
    /// Line count for the top contributor.
    pub top_contributor_lines: i64,
}

/// A high-churn file flagged as a hotspot.
#[derive(Debug, Clone, PartialEq)]
pub struct HotspotData {
    /// File path.
    pub path: String,
    /// Number of commits.
    pub commit_count: i64,
    /// Composite churn score.
    pub churn_score: f64,
    /// Risk level string (`CRITICAL`/`HIGH`/`MEDIUM`).
    pub risk_level: String,
}

/// Repository-wide summary statistics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AggregateData {
    /// Total number of files.
    pub total_files: i64,
    /// Total number of commits across all files.
    pub total_commits: i64,
    /// Number of distinct contributors.
    pub total_contributors: i64,
    /// Average commits per file.
    pub avg_commits_per_file: f64,
    /// Average contributors per file.
    pub avg_contributors_per_file: f64,
    /// Number of files at/above the medium churn threshold.
    pub high_churn_files: i64,
}

/// Aggregate file composition breakdown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompositionData {
    /// Category name -> count (only non-zero categories present).
    pub breakdown: BTreeMap<String, i64>,
    /// Category name -> percentage of total.
    pub percentages: BTreeMap<String, f64>,
}

/// File composition for a single tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompositionTimeSeriesEntry {
    /// Tick index.
    pub tick: i64,
    /// Tick start time (RFC3339), empty when unknown (`omitempty`).
    pub start_time: String,
    /// Tick end time (RFC3339), empty when unknown (`omitempty`).
    pub end_time: String,
    /// Category name -> count for this tick (only non-zero categories present).
    pub breakdown: BTreeMap<String, i64>,
}

/// All computed metric results.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// Per-file churn, sorted by churn score descending.
    pub file_churn: Vec<FileChurnData>,
    /// Per-file contributor breakdown.
    pub file_contributors: Vec<FileContributorData>,
    /// Hotspot files, sorted by risk then commit count.
    pub hotspots: Vec<HotspotData>,
    /// Aggregate summary.
    pub aggregate: AggregateData,
    /// Aggregate composition.
    pub composition: CompositionData,
    /// Per-tick composition time series.
    pub composition_ts: Vec<CompositionTimeSeriesEntry>,
}

impl ComputedMetrics {
    /// Returns the analyzer identifier.
    #[must_use]
    pub fn analyzer_name(&self) -> &'static str {
        crate::ANALYZER_NAME
    }
}

/// Configurable hotspot thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricOptions {
    /// Critical commit threshold.
    pub hotspot_threshold_critical: i64,
    /// High commit threshold.
    pub hotspot_threshold_high: i64,
    /// Medium commit threshold.
    pub hotspot_threshold_medium: i64,
}

impl Default for MetricOptions {
    fn default() -> Self {
        Self {
            hotspot_threshold_critical: HOTSPOT_THRESHOLD_CRITICAL,
            hotspot_threshold_high: HOTSPOT_THRESHOLD_HIGH,
            hotspot_threshold_medium: HOTSPOT_THRESHOLD_MEDIUM,
        }
    }
}

/// Computes all metrics with default options.
#[must_use]
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    let empty: BTreeMap<i64, CategoryCounts> = BTreeMap::new();
    compute_all_metrics_with_options(input, MetricOptions::default(), &empty, None)
}

/// Tick bounds (start/end RFC3339 strings) keyed by tick index.
pub type TickBounds = BTreeMap<i64, (String, String)>;

/// Computes all metrics with configurable thresholds and optional composition
/// inputs.
///
/// `tick_composition` and `tick_bounds` correspond to the `tick_composition`
/// and `tick_bounds` report entries; pass an empty map / `None` when not
/// available.
#[must_use]
pub fn compute_all_metrics_with_options(
    input: &ReportData,
    opts: MetricOptions,
    tick_composition: &BTreeMap<i64, CategoryCounts>,
    tick_bounds: Option<&TickBounds>,
) -> ComputedMetrics {
    let (composition, composition_ts) = compute_composition(tick_composition, tick_bounds);
    ComputedMetrics {
        file_churn: compute_file_churn(input),
        file_contributors: compute_file_contributors(input),
        hotspots: compute_hotspots_with_options(input, opts),
        aggregate: compute_aggregate_with_options(input, opts),
        composition,
        composition_ts,
    }
}

/// Computes per-file churn data, sorted by churn score descending.
///
/// ```
/// use cf_file_history::{FileChurnData, FileHistory, LineStats, ReportData};
/// use cf_file_history::metrics::compute_file_churn;
/// use std::collections::BTreeMap;
///
/// // "hot.rs" has far more activity than "quiet.rs".
/// let hot = FileHistory {
///     people: BTreeMap::from([(1, LineStats { added: 500, removed: 200, changed: 100 })]),
///     hashes: (0..40).map(|i| format!("{i:040}")).collect(),
/// };
/// let quiet = FileHistory {
///     people: BTreeMap::from([(2, LineStats { added: 5, removed: 1, changed: 0 })]),
///     hashes: vec!["a".repeat(40)],
/// };
/// let input = ReportData {
///     files: BTreeMap::from([("quiet.rs".into(), quiet), ("hot.rs".into(), hot)]),
/// };
///
/// let churn = compute_file_churn(&input);
/// // Sorted by churn score descending: the hot file comes first.
/// assert_eq!(churn[0].path, "hot.rs");
/// assert_eq!(churn[0].commit_count, 40);
/// assert_eq!(churn[0].contributor_count, 1);
/// assert!(churn[0].churn_score > churn[1].churn_score);
/// ```
#[must_use]
pub fn compute_file_churn(input: &ReportData) -> Vec<FileChurnData> {
    let mut result: Vec<FileChurnData> = input
        .files
        .iter()
        .map(|(path, fh)| {
            let (total_added, total_removed, total_changed) = sum_line_stats(fh);
            let commit_count = fh.hashes.len() as i64;
            let contributor_count = fh.people.len() as i64;
            let churn_score = churn_score(commit_count, total_added, total_removed, total_changed);
            FileChurnData {
                path: path.clone(),
                commit_count,
                contributor_count,
                total_added,
                total_removed,
                total_changed,
                churn_score,
            }
        })
        .collect();

    sort_by_churn_desc(&mut result);
    result
}

/// Computes per-file contributor breakdowns.
#[must_use]
pub fn compute_file_contributors(input: &ReportData) -> Vec<FileContributorData> {
    input
        .files
        .iter()
        .map(|(path, fh)| {
            let mut top_id = 0i64;
            let mut top_lines = 0i64;
            let mut contribs: Vec<ContributorEntry> = fh
                .people
                .iter()
                .map(|(&dev_id, stats)| {
                    let total_lines = stats.added + stats.changed;
                    if total_lines > top_lines {
                        top_lines = total_lines;
                        top_id = dev_id;
                    }
                    ContributorEntry {
                        dev_id,
                        added: stats.added,
                        removed: stats.removed,
                        changed: stats.changed,
                    }
                })
                .collect();

            contribs.sort_by(|a, b| a.dev_id.cmp(&b.dev_id));

            FileContributorData {
                path: path.clone(),
                contributors: contribs,
                top_contributor_id: top_id,
                top_contributor_lines: top_lines,
            }
        })
        .collect()
}

/// Computes hotspot files, sorted by risk then commit count.
#[must_use]
pub fn compute_hotspots_with_options(input: &ReportData, opts: MetricOptions) -> Vec<HotspotData> {
    let critical = opts.hotspot_threshold_critical;
    let high = opts.hotspot_threshold_high;
    let medium = opts.hotspot_threshold_medium;

    let mut result: Vec<HotspotData> = Vec::new();

    for (path, fh) in &input.files {
        let commit_count = fh.hashes.len() as i64;
        let (total_added, total_removed, total_changed) = sum_line_stats(fh);
        let churn_score = churn_score(commit_count, total_added, total_removed, total_changed);

        // Risk levels are the uppercase report tokens (CRITICAL/HIGH/MEDIUM).
        let risk_level = if commit_count >= critical {
            RiskLevel::critical()
        } else if commit_count >= high {
            RiskLevel::high()
        } else if commit_count >= medium {
            RiskLevel::medium()
        } else {
            continue; // skip low-risk files
        };

        result.push(HotspotData {
            path: path.clone(),
            commit_count,
            churn_score,
            risk_level: risk_level.as_str().to_string(),
        });
    }

    // Sort by risk (critical first) then by commit count descending.
    result.sort_by(|a, b| {
        if a.risk_level == b.risk_level {
            b.commit_count.cmp(&a.commit_count)
        } else {
            let ip = risk_priority(&RiskLevel::from(a.risk_level.as_str()));
            let jp = risk_priority(&RiskLevel::from(b.risk_level.as_str()));
            ip.cmp(&jp)
        }
    });

    result
}

/// Computes aggregate statistics.
#[must_use]
pub fn compute_aggregate_with_options(input: &ReportData, opts: MetricOptions) -> AggregateData {
    let total_files = input.files.len() as i64;
    let mut agg = AggregateData { total_files, ..Default::default() };

    if total_files == 0 {
        return agg;
    }

    let medium = opts.hotspot_threshold_medium;
    let mut all_contributors: BTreeMap<i64, bool> = BTreeMap::new();
    let mut total_commits = 0i64;
    let mut high_churn_count = 0i64;

    for fh in input.files.values() {
        let commit_count = fh.hashes.len() as i64;
        total_commits += commit_count;
        for &dev_id in fh.people.keys() {
            all_contributors.insert(dev_id, true);
        }
        if commit_count >= medium {
            high_churn_count += 1;
        }
    }

    agg.total_commits = total_commits;
    agg.total_contributors = all_contributors.len() as i64;
    agg.high_churn_files = high_churn_count;
    agg.avg_commits_per_file = total_commits as f64 / total_files as f64;

    let total_contributor_count: i64 = input.files.values().map(|fh| fh.people.len() as i64).sum();
    agg.avg_contributors_per_file = total_contributor_count as f64 / total_files as f64;

    agg
}

/// Computes the aggregate and per-tick composition.
#[must_use]
pub fn compute_composition(
    tick_comp: &BTreeMap<i64, CategoryCounts>,
    tick_bounds: Option<&TickBounds>,
) -> (CompositionData, Vec<CompositionTimeSeriesEntry>) {
    let mut comp = CompositionData::default();

    if tick_comp.is_empty() {
        return (comp, Vec::new());
    }

    // BTreeMap iterates ticks in ascending order (report contract).
    let mut ts: Vec<CompositionTimeSeriesEntry> = Vec::with_capacity(tick_comp.len());
    let mut total = CategoryCounts::default();

    for (&t, counts) in tick_comp {
        total.add(counts);

        let mut breakdown = BTreeMap::new();
        for cat in ALL_CATEGORIES {
            let v = counts.get(cat);
            if v > 0 {
                breakdown.insert(cat.as_str().to_string(), v);
            }
        }

        let mut entry = CompositionTimeSeriesEntry { tick: t, breakdown, ..Default::default() };
        if let Some(bounds) = tick_bounds.and_then(|tb| tb.get(&t)) {
            entry.start_time.clone_from(&bounds.0);
            entry.end_time.clone_from(&bounds.1);
        }
        ts.push(entry);
    }

    let grand_total = total.total();
    for cat in ALL_CATEGORIES {
        let v = total.get(cat);
        if v > 0 {
            comp.breakdown.insert(cat.as_str().to_string(), v);
        }
        if grand_total > 0 {
            comp.percentages
                .insert(cat.as_str().to_string(), v as f64 / grand_total as f64 * PERCENT_MULTIPLIER);
        }
    }

    (comp, ts)
}

// --- helpers ---

/// Sums per-author line stats for a file: `(added, removed, changed)`.
fn sum_line_stats(fh: &FileHistory) -> (i64, i64, i64) {
    fh.people.values().fold((0, 0, 0), |(a, r, c), stats| {
        (a + stats.added, r + stats.removed, c + stats.changed)
    })
}

fn churn_score(commit_count: i64, added: i64, removed: i64, changed: i64) -> f64 {
    commit_count as f64 + (added + removed + changed) as f64 / CHURN_SCORE_DIVISOR
}

/// Sorts churn data by churn score descending. Ties keep an unspecified order
/// (the report contract uses the same comparator, so the numeric values are
/// identical).
fn sort_by_churn_desc(v: &mut [FileChurnData]) {
    v.sort_by(|a, b| {
        b.churn_score
            .partial_cmp(&a.churn_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE1: &str = "file1.go";
    const FILE2: &str = "file2.go";
    const FILE3: &str = "file3.go";
    const DEV1: i64 = 1;
    const DEV2: i64 = 2;
    const DEV3: i64 = 3;
    const DELTA: f64 = 0.01;

    fn hashes(count: usize) -> Vec<String> {
        // Distinct entries; only the length matters.
        (0..count).map(|i| format!("h{i}")).collect()
    }

    fn people(entries: &[(i64, LineStats)]) -> BTreeMap<i64, LineStats> {
        entries.iter().copied().collect()
    }

    fn report(files: &[(&str, FileHistory)]) -> ReportData {
        ReportData {
            files: files.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect(),
        }
    }

    #[test]
    fn parse_report_data_empty() {
        let input = ReportData::default();
        assert!(input.files.is_empty());
    }

    #[test]
    fn file_churn_empty() {
        let input = ReportData::default();
        assert!(compute_file_churn(&input).is_empty());
    }

    #[test]
    fn file_churn_single_file() {
        let input = report(&[(
            FILE1,
            FileHistory {
                people: people(&[
                    (DEV1, LineStats { added: 100, removed: 20, changed: 30 }),
                    (DEV2, LineStats { added: 50, removed: 10, changed: 15 }),
                ]),
                hashes: hashes(10),
            },
        )]);
        let result = compute_file_churn(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, FILE1);
        assert_eq!(result[0].commit_count, 10);
        assert_eq!(result[0].contributor_count, 2);
        assert_eq!(result[0].total_added, 150);
        assert_eq!(result[0].total_removed, 30);
        assert_eq!(result[0].total_changed, 45);
        assert!((result[0].churn_score - 12.25).abs() < DELTA);
    }

    #[test]
    fn file_churn_sorted_by_score() {
        let input = report(&[
            (FILE1, FileHistory { people: people(&[(DEV1, LineStats { added: 10, ..Default::default() })]), hashes: hashes(5) }),
            (FILE2, FileHistory { people: people(&[(DEV1, LineStats { added: 1000, ..Default::default() })]), hashes: hashes(20) }),
            (FILE3, FileHistory { people: people(&[(DEV1, LineStats { added: 100, ..Default::default() })]), hashes: hashes(10) }),
        ]);
        let result = compute_file_churn(&input);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].path, FILE2);
        assert_eq!(result[1].path, FILE3);
        assert_eq!(result[2].path, FILE1);
    }

    #[test]
    fn contributors_single_file() {
        let input = report(&[(
            FILE1,
            FileHistory {
                people: people(&[
                    (DEV1, LineStats { added: 50, changed: 20, ..Default::default() }),
                    (DEV2, LineStats { added: 100, changed: 30, ..Default::default() }),
                ]),
                hashes: hashes(5),
            },
        )]);
        let result = compute_file_contributors(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contributors.len(), 2);
        assert_eq!(result[0].top_contributor_id, DEV2);
        assert_eq!(result[0].top_contributor_lines, 130);
    }

    #[test]
    fn contributors_none() {
        let input = report(&[(FILE1, FileHistory { people: BTreeMap::new(), hashes: hashes(5) })]);
        let result = compute_file_contributors(&input);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].top_contributor_id, 0);
        assert_eq!(result[0].top_contributor_lines, 0);
    }

    #[test]
    fn hotspots_empty() {
        let input = ReportData::default();
        assert!(compute_hotspots_with_options(&input, MetricOptions::default()).is_empty());
    }

    #[test]
    fn hotspots_below_threshold() {
        let input = report(&[(FILE1, FileHistory { people: people(&[(DEV1, LineStats::default())]), hashes: hashes(10) })]);
        assert!(compute_hotspots_with_options(&input, MetricOptions::default()).is_empty());
    }

    #[test]
    fn hotspots_risk_levels() {
        let cases = [
            (55, "CRITICAL"),
            (50, "CRITICAL"),
            (35, "HIGH"),
            (30, "HIGH"),
            (20, "MEDIUM"),
            (15, "MEDIUM"),
        ];
        for (count, expected) in cases {
            let input = report(&[(FILE1, FileHistory { people: people(&[(DEV1, LineStats::default())]), hashes: hashes(count) })]);
            let result = compute_hotspots_with_options(&input, MetricOptions::default());
            assert_eq!(result.len(), 1, "count={count}");
            assert_eq!(result[0].risk_level, expected, "count={count}");
            assert_eq!(result[0].commit_count, count as i64);
        }
    }

    #[test]
    fn hotspots_sorted_by_risk_then_count() {
        let input = report(&[
            (FILE1, FileHistory { people: people(&[(DEV1, LineStats::default())]), hashes: hashes(20) }),
            (FILE2, FileHistory { people: people(&[(DEV1, LineStats::default())]), hashes: hashes(55) }),
            (FILE3, FileHistory { people: people(&[(DEV1, LineStats::default())]), hashes: hashes(35) }),
        ]);
        let result = compute_hotspots_with_options(&input, MetricOptions::default());
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].risk_level, "CRITICAL");
        assert_eq!(result[1].risk_level, "HIGH");
        assert_eq!(result[2].risk_level, "MEDIUM");
    }

    #[test]
    fn aggregate_empty() {
        let input = ReportData::default();
        let r = compute_aggregate_with_options(&input, MetricOptions::default());
        assert_eq!(r, AggregateData::default());
    }

    #[test]
    fn aggregate_with_data() {
        let input = report(&[
            (FILE1, FileHistory { people: people(&[(DEV1, LineStats::default()), (DEV2, LineStats::default())]), hashes: hashes(20) }),
            (FILE2, FileHistory { people: people(&[(DEV1, LineStats::default()), (DEV3, LineStats::default())]), hashes: hashes(10) }),
        ]);
        let r = compute_aggregate_with_options(&input, MetricOptions::default());
        assert_eq!(r.total_files, 2);
        assert_eq!(r.total_commits, 30);
        assert_eq!(r.total_contributors, 3);
        assert!((r.avg_commits_per_file - 15.0).abs() < DELTA);
        assert!((r.avg_contributors_per_file - 2.0).abs() < DELTA);
        assert_eq!(r.high_churn_files, 1);
    }

    #[test]
    fn compute_all_empty() {
        let input = ReportData::default();
        let r = compute_all_metrics(&input);
        assert!(r.file_churn.is_empty());
        assert!(r.file_contributors.is_empty());
        assert!(r.hotspots.is_empty());
        assert_eq!(r.aggregate.total_files, 0);
    }

    #[test]
    fn computed_metrics_analyzer_name() {
        assert_eq!(ComputedMetrics::default().analyzer_name(), "file_history");
    }

    #[test]
    fn compute_all_full() {
        let input = report(&[
            (FILE1, FileHistory {
                people: people(&[
                    (DEV1, LineStats { added: 100, removed: 10, changed: 20 }),
                    (DEV2, LineStats { added: 50, removed: 5, changed: 10 }),
                ]),
                hashes: hashes(35),
            }),
            (FILE2, FileHistory { people: people(&[(DEV1, LineStats { added: 20, ..Default::default() })]), hashes: hashes(5) }),
        ]);
        let r = compute_all_metrics(&input);
        assert_eq!(r.file_churn.len(), 2);
        assert_eq!(r.file_contributors.len(), 2);
        assert_eq!(r.hotspots.len(), 1);
        assert_eq!(r.hotspots[0].path, FILE1);
        assert_eq!(r.hotspots[0].risk_level, "HIGH");
        assert_eq!(r.aggregate.total_files, 2);
        assert_eq!(r.aggregate.total_commits, 40);
        assert_eq!(r.aggregate.total_contributors, 2);
    }

    #[test]
    fn composition_basic() {
        let mut tick_comp = BTreeMap::new();
        tick_comp.insert(0, CategoryCounts { source: 3, vendor: 1, ..Default::default() });
        let (comp, ts) = compute_composition(&tick_comp, None);
        assert_eq!(comp.breakdown.get("source"), Some(&3));
        assert_eq!(comp.breakdown.get("vendor"), Some(&1));
        // 3/4 * 100 = 75, 1/4 * 100 = 25.
        assert!((comp.percentages["source"] - 75.0).abs() < DELTA);
        assert!((comp.percentages["vendor"] - 25.0).abs() < DELTA);
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].tick, 0);
        assert_eq!(ts[0].breakdown.get("source"), Some(&3));
    }
}

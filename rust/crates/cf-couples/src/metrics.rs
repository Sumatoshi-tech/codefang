//! Couples metrics computation (port of `metrics.go`).
//!
//! These types and functions are the data-parity-critical core: they produce
//! the numbers that appear in the machine report. The coupling-strength formula
//! is code-maat's `co_changes / avg(revs_a, revs_b)` capped at `1.0`, where
//! `revs` is the diagonal (self-change) count.

use std::collections::BTreeMap;

/// Divisor when averaging two revision counts (Go: `pairCount`).
const PAIR_COUNT: f64 = 2.0;

/// Coupling-strength threshold for the "highly coupled" count
/// (Go: `CouplingThresholdHigh`).
pub const COUPLING_THRESHOLD_HIGH: i64 = 10;

/// Default HLL precision for contributor cardinality (Go:
/// `fileContribHLLPrecision`). 1024 registers, ~3% error.
pub const FILE_CONTRIB_HLL_PRECISION: u8 = 10;

const OWNERSHIP_FEW_THRESHOLD: i32 = 3;
const OWNERSHIP_MODERATE_THRESHOLD: i32 = 5;

/// Parsed analyzer-report inputs for metric computation (Go: `ReportData`).
#[derive(Debug, Clone, Default)]
pub struct ReportData {
    /// Developer co-occurrence matrix (index = dev, map dev → shared count).
    pub people_matrix: Vec<BTreeMap<usize, i64>>,
    /// Per-developer sorted file-index lists.
    pub people_files: Vec<Vec<usize>>,
    /// Byte-sorted file names.
    pub files: Vec<String>,
    /// Per-file line counts (parallel to `files`).
    pub files_lines: Vec<i32>,
    /// File co-occurrence matrix (index = file, map file → count).
    pub files_matrix: Vec<BTreeMap<usize, i64>>,
    /// Developer index → `"name|email"` identity strings.
    pub reversed_people_dict: Vec<String>,
}

/// Coupling data for a file pair (Go: `FileCouplingData`).
///
/// JSON/YAML tags: `file1`, `file2`, `co_changes`, `coupling_strength`.
#[derive(Debug, Clone, PartialEq)]
pub struct FileCouplingData {
    pub file1: String,
    pub file2: String,
    pub co_changes: i64,
    pub strength: f64,
}

/// Coupling data for a developer pair (Go: `DeveloperCouplingData`).
///
/// JSON/YAML tags: `developer1`, `developer1_email` (omitempty), `developer2`,
/// `developer2_email` (omitempty), `shared_file_changes`, `coupling_strength`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeveloperCouplingData {
    pub developer1: String,
    pub developer1_email: String,
    pub developer2: String,
    pub developer2_email: String,
    pub shared_files: i64,
    pub strength: f64,
}

/// Ownership information for a file (Go: `FileOwnershipData`).
///
/// JSON/YAML tags: `file`, `lines`, `contributors`, `top_contributor`
/// (omitempty).
#[derive(Debug, Clone, PartialEq)]
pub struct FileOwnershipData {
    pub file: String,
    pub lines: i32,
    pub contributors: i32,
    pub top_contributor: String,
}

/// Aggregate summary statistics (Go: `AggregateData`).
///
/// JSON/YAML tags: `total_files`, `total_developers`, `total_co_changes`,
/// `avg_coupling_strength`, `highly_coupled_pairs`.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateData {
    pub total_files: i32,
    pub total_developers: i32,
    pub total_co_changes: i64,
    pub avg_coupling_strength: f64,
    pub highly_coupled_pairs: i32,
}

impl Default for AggregateData {
    fn default() -> Self {
        AggregateData {
            total_files: 0,
            total_developers: 0,
            total_co_changes: 0,
            avg_coupling_strength: 0.0,
            highly_coupled_pairs: 0,
        }
    }
}

/// A contributor-count distribution bucket (Go: `OwnershipBucket`).
///
/// JSON/YAML tags: `label`, `count`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipBucket {
    pub label: String,
    pub count: i32,
}

/// Configurable thresholds for metric computation (Go: `MetricOptions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricOptions {
    pub coupling_threshold_high: i32,
    pub ownership_few_threshold: i32,
    pub ownership_moderate_threshold: i32,
    pub batch_coupling_threshold: i32,
    pub hll_precision: i32,
}

impl Default for MetricOptions {
    /// Go: `DefaultMetricOptions`.
    fn default() -> Self {
        MetricOptions {
            coupling_threshold_high: COUPLING_THRESHOLD_HIGH as i32,
            ownership_few_threshold: OWNERSHIP_FEW_THRESHOLD,
            ownership_moderate_threshold: OWNERSHIP_MODERATE_THRESHOLD,
            batch_coupling_threshold: 0,
            hll_precision: FILE_CONTRIB_HLL_PRECISION as i32,
        }
    }
}

/// All computed metric results (Go: `ComputedMetrics`).
///
/// JSON/YAML tags: `file_coupling`, `developer_coupling`, `file_ownership`,
/// `aggregate`.
#[derive(Debug, Clone, Default)]
pub struct ComputedMetrics {
    pub file_coupling: Vec<FileCouplingData>,
    pub developer_coupling: Vec<DeveloperCouplingData>,
    pub file_ownership: Vec<FileOwnershipData>,
    pub aggregate: AggregateData,
}

/// Coupling strength: `co_changes / avg(self_i, self_j)`, capped at `1.0`.
///
/// Public re-export of [`coupling_strength`] for the [`crate::store`] module so
/// the sparse store path reuses the identical formula.
pub fn coupling_strength_pub(co_changes: i64, self_i: i64, self_j: i64) -> f64 {
    coupling_strength(co_changes, self_i, self_j)
}

/// Coupling strength: `co_changes / avg(self_i, self_j)`, capped at `1.0`.
///
/// Mirrors the Go expression `min(float64(coChanges)/avgRevs, 1.0)` with the
/// `avgRevs <= 0 → 0.0` guard. Shared by every metric.
fn coupling_strength(co_changes: i64, self_i: i64, self_j: i64) -> f64 {
    let avg_revs = (self_i + self_j) as f64 / PAIR_COUNT;
    if avg_revs > 0.0 {
        (co_changes as f64 / avg_revs).min(1.0)
    } else {
        0.0
    }
}

/// Computes file coupling pairs from the dense files matrix
/// (Go: `FileCouplingMetric.Compute`).
///
/// Iterates the upper triangle (`j > i`), skips zero co-changes, and sorts the
/// result by `co_changes` descending. The Go code uses `sort.Slice`, which is
/// **not** stable; this port uses a stable sort, which can differ in tie
/// ordering — see crate TODOs on matching Go's unstable sort for byte-identity.
pub fn compute_file_coupling(input: &ReportData) -> Vec<FileCouplingData> {
    let mut result: Vec<FileCouplingData> = Vec::new();
    for (i, row) in input.files_matrix.iter().enumerate() {
        if i >= input.files.len() {
            continue;
        }
        let file1 = &input.files[i];
        let self_i = *row.get(&i).unwrap_or(&0);
        for (&j, &co_changes) in row {
            if j <= i || j >= input.files.len() {
                continue;
            }
            if co_changes == 0 {
                continue;
            }
            let self_j = input
                .files_matrix
                .get(j)
                .and_then(|r| r.get(&j))
                .copied()
                .unwrap_or(0);
            result.push(FileCouplingData {
                file1: file1.clone(),
                file2: input.files[j].clone(),
                co_changes,
                strength: coupling_strength(co_changes, self_i, self_j),
            });
        }
    }
    result.sort_by(|a, b| b.co_changes.cmp(&a.co_changes));
    result
}

/// Computes developer coupling pairs (Go: `DeveloperCouplingMetric.Compute`).
///
/// Upper triangle over the people matrix; sorted by `shared_files` descending.
pub fn compute_developer_coupling(input: &ReportData) -> Vec<DeveloperCouplingData> {
    let names = &input.reversed_people_dict;
    let mut result: Vec<DeveloperCouplingData> = Vec::new();
    for (dev_idx, row) in input.people_matrix.iter().enumerate() {
        let (d1_name, d1_email) = dev_name_email(dev_idx, names);
        let self_dev1 = *row.get(&dev_idx).unwrap_or(&0);
        for (&j, &shared) in row {
            if j <= dev_idx || shared == 0 {
                continue;
            }
            let self_dev2 = input
                .people_matrix
                .get(j)
                .and_then(|r| r.get(&j))
                .copied()
                .unwrap_or(0);
            let (d2_name, d2_email) = dev_name_email(j, names);
            result.push(DeveloperCouplingData {
                developer1: d1_name.clone(),
                developer1_email: d1_email.clone(),
                developer2: d2_name,
                developer2_email: d2_email,
                shared_files: shared,
                strength: coupling_strength(shared, self_dev1, self_dev2),
            });
        }
    }
    result.sort_by(|a, b| b.shared_files.cmp(&a.shared_files));
    result
}

fn dev_name_email(idx: usize, names: &[String]) -> (String, String) {
    if idx < names.len() {
        crate::split_identity(&names[idx])
    } else {
        (String::new(), String::new())
    }
}

/// Computes file ownership with exact contributor counts.
///
/// Go uses per-file HyperLogLog sketches keyed by `LittleEndian(devID)` for
/// memory efficiency on large repos; with the `hll` feature this uses the same
/// sketch so counts match exactly, otherwise it counts distinct developer IDs
/// exactly via a set (parity-equivalent for typical inputs, exact for small
/// ones). See crate TODOs.
pub fn compute_file_ownership(input: &ReportData, opts: MetricOptions) -> Vec<FileOwnershipData> {
    let contributors = file_contributor_counts(input.files.len(), &input.people_files, opts.hll_precision as u8);
    let mut result = Vec::with_capacity(input.files.len());
    for (i, file) in input.files.iter().enumerate() {
        let lines = input.files_lines.get(i).copied().unwrap_or(0);
        result.push(FileOwnershipData {
            file: file.clone(),
            lines,
            contributors: contributors.get(i).copied().unwrap_or(0),
            top_contributor: String::new(),
        });
    }
    result
}

/// Per-file contributor cardinality, indexed by file.
#[cfg(feature = "hll")]
fn file_contributor_counts(num_files: usize, people_files: &[Vec<usize>], precision: u8) -> Vec<i32> {
    use cf_alg_hll::Sketch;
    let mut sketches: Vec<Option<Sketch>> = (0..num_files)
        .map(|_| Sketch::new(precision).ok())
        .collect();
    for (dev_id, file_indices) in people_files.iter().enumerate() {
        let dev_buf = (dev_id as u64).to_le_bytes();
        for &fi in file_indices {
            if let Some(Some(s)) = sketches.get_mut(fi) {
                s.add(&dev_buf);
            }
        }
    }
    sketches
        .iter()
        .map(|s| s.as_ref().map(|s| s.count() as i32).unwrap_or(0))
        .collect()
}

/// Exact-count fallback used without the `hll` feature.
#[cfg(not(feature = "hll"))]
fn file_contributor_counts(num_files: usize, people_files: &[Vec<usize>], _precision: u8) -> Vec<i32> {
    use std::collections::HashSet;
    let mut sets: Vec<HashSet<usize>> = vec![HashSet::new(); num_files];
    for (dev_id, file_indices) in people_files.iter().enumerate() {
        for &fi in file_indices {
            if fi < num_files {
                sets[fi].insert(dev_id);
            }
        }
    }
    sets.iter().map(|s| s.len() as i32).collect()
}

/// Computes aggregate statistics from the dense files matrix
/// (Go: `AggregateMetric.ComputeWithOptions`).
pub fn compute_aggregate(input: &ReportData, opts: MetricOptions) -> AggregateData {
    let mut total_co_changes: i64 = 0;
    let mut pair_count: i64 = 0;
    let mut highly_coupled: i32 = 0;
    let mut total_strength: f64 = 0.0;
    let threshold = opts.coupling_threshold_high as i64;

    for (i, row) in input.files_matrix.iter().enumerate() {
        let self_i = *row.get(&i).unwrap_or(&0);
        for (&j, &co_changes) in row {
            if j <= i || co_changes <= 0 {
                continue;
            }
            let self_j = input
                .files_matrix
                .get(j)
                .and_then(|r| r.get(&j))
                .copied()
                .unwrap_or(0);
            total_co_changes += co_changes;
            pair_count += 1;
            if co_changes >= threshold {
                highly_coupled += 1;
            }
            total_strength += coupling_strength(co_changes, self_i, self_j);
        }
    }

    AggregateData {
        total_files: input.files.len() as i32,
        total_developers: input.reversed_people_dict.len() as i32,
        total_co_changes,
        avg_coupling_strength: if pair_count > 0 {
            total_strength / pair_count as f64
        } else {
            0.0
        },
        highly_coupled_pairs: highly_coupled,
    }
}

/// Groups ownership data into contributor-count buckets
/// (Go: `BucketOwnership` / `BucketOwnershipWithThresholds`).
pub fn bucket_ownership(ownership: &[FileOwnershipData]) -> Vec<OwnershipBucket> {
    bucket_ownership_with_thresholds(ownership, OWNERSHIP_FEW_THRESHOLD, OWNERSHIP_MODERATE_THRESHOLD)
}

/// Groups ownership data with configurable thresholds.
pub fn bucket_ownership_with_thresholds(
    ownership: &[FileOwnershipData],
    few_threshold: i32,
    moderate_threshold: i32,
) -> Vec<OwnershipBucket> {
    let (mut single, mut few, mut moderate, mut many) = (0, 0, 0, 0);
    for fo in ownership {
        if fo.contributors <= 1 {
            single += 1;
        } else if fo.contributors <= few_threshold {
            few += 1;
        } else if fo.contributors <= moderate_threshold {
            moderate += 1;
        } else {
            many += 1;
        }
    }
    vec![
        OwnershipBucket { label: "Single owner".to_string(), count: single },
        OwnershipBucket { label: format!("2-{few_threshold} owners"), count: few },
        OwnershipBucket {
            label: format!("{}-{} owners", few_threshold + 1, moderate_threshold),
            count: moderate,
        },
        OwnershipBucket { label: format!("{}+ owners", moderate_threshold + 1), count: many },
    ]
}

/// Returns a copy sorted by contributors ascending (highest risk first)
/// (Go: `SortOwnershipByRisk`).
pub fn sort_ownership_by_risk(ownership: &[FileOwnershipData]) -> Vec<FileOwnershipData> {
    let mut sorted = ownership.to_vec();
    sorted.sort_by(|a, b| a.contributors.cmp(&b.contributors));
    sorted
}

/// Limits a developer matrix to the top-N developers by diagonal activity
/// (Go: `FilterTopDevs`). Returns the input unchanged when within the limit.
pub fn filter_top_devs(
    matrix: &[BTreeMap<usize, i64>],
    names: &[String],
    limit: usize,
) -> (Vec<BTreeMap<usize, i64>>, Vec<String>) {
    if names.len() <= limit {
        return (matrix.to_vec(), names.to_vec());
    }
    let mut devs: Vec<(usize, i64)> = (0..names.len())
        .map(|i| (i, matrix[i].get(&i).copied().unwrap_or(0)))
        .collect();
    devs.sort_by(|a, b| b.1.cmp(&a.1));
    let top_n = &devs[..limit];

    let mut old_to_new = std::collections::HashMap::with_capacity(limit);
    let mut new_names = vec![String::new(); limit];
    for (new_idx, &(old_idx, _)) in top_n.iter().enumerate() {
        old_to_new.insert(old_idx, new_idx);
        new_names[new_idx] = names[old_idx].clone();
    }

    let mut new_matrix: Vec<BTreeMap<usize, i64>> = vec![BTreeMap::new(); limit];
    for &(old_i, _) in top_n {
        let new_i = old_to_new[&old_i];
        for (&old_j, &val) in &matrix[old_i] {
            if let Some(&new_j) = old_to_new.get(&old_j) {
                new_matrix[new_i].insert(new_j, val);
            }
        }
    }
    (new_matrix, new_names)
}

/// Runs all metrics with default options (Go: `ComputeAllMetrics`).
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    compute_all_metrics_with_options(input, MetricOptions::default())
}

/// Runs all metrics with configurable thresholds
/// (Go: `ComputeAllMetricsWithOptions`).
pub fn compute_all_metrics_with_options(input: &ReportData, opts: MetricOptions) -> ComputedMetrics {
    ComputedMetrics {
        file_coupling: compute_file_coupling(input),
        developer_coupling: compute_developer_coupling(input),
        file_ownership: compute_file_ownership(input, opts),
        aggregate: compute_aggregate(input, opts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pairs: &[(usize, i64)]) -> BTreeMap<usize, i64> {
        pairs.iter().copied().collect()
    }

    fn sample() -> ReportData {
        // Two files a,b. a self=4, b self=2, a-b co-change=2.
        ReportData {
            files: vec!["a.go".into(), "b.go".into()],
            files_lines: vec![10, 20],
            files_matrix: vec![row(&[(0, 4), (1, 2)]), row(&[(1, 2), (0, 2)])],
            // Symmetric people matrix (as accumulate_matrix produces it):
            // dev0 self=3, dev1 self=5, dev0<->dev1 shared=2.
            people_matrix: vec![row(&[(0, 3), (1, 2)]), row(&[(1, 5), (0, 2)])],
            people_files: vec![vec![0], vec![0, 1]],
            reversed_people_dict: vec!["Alice|a@x".into(), "Bob|b@x".into()],
        }
    }

    #[test]
    fn strength_formula() {
        // co=2, self_i=4, self_j=2 -> avg=3 -> 2/3.
        let s = coupling_strength(2, 4, 2);
        assert!((s - 2.0 / 3.0).abs() < 1e-12);
        // capped at 1.0.
        assert_eq!(coupling_strength(10, 2, 2), 1.0);
        // zero avg guard.
        assert_eq!(coupling_strength(5, 0, 0), 0.0);
    }

    #[test]
    fn file_coupling_upper_triangle() {
        let fc = compute_file_coupling(&sample());
        assert_eq!(fc.len(), 1);
        assert_eq!(fc[0].file1, "a.go");
        assert_eq!(fc[0].file2, "b.go");
        assert_eq!(fc[0].co_changes, 2);
        assert!((fc[0].strength - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn developer_coupling_splits_identity() {
        let dc = compute_developer_coupling(&sample());
        assert_eq!(dc.len(), 1);
        assert_eq!(dc[0].developer1, "Alice");
        assert_eq!(dc[0].developer1_email, "a@x");
        assert_eq!(dc[0].developer2, "Bob");
        assert_eq!(dc[0].shared_files, 2);
        // avg(self0=3, self1=5)=4 -> 2/4 = 0.5.
        assert_eq!(dc[0].strength, 0.5);
    }

    #[test]
    fn ownership_exact_contributor_counts() {
        let own = compute_file_ownership(&sample(), MetricOptions::default());
        // file a (idx0) touched by dev0 and dev1 -> 2 contributors.
        assert_eq!(own[0].file, "a.go");
        assert_eq!(own[0].lines, 10);
        assert_eq!(own[0].contributors, 2);
        // file b (idx1) touched by dev1 only -> 1 contributor.
        assert_eq!(own[1].contributors, 1);
    }

    #[test]
    fn aggregate_counts() {
        let agg = compute_aggregate(&sample(), MetricOptions::default());
        assert_eq!(agg.total_files, 2);
        assert_eq!(agg.total_developers, 2);
        assert_eq!(agg.total_co_changes, 2);
        assert_eq!(agg.highly_coupled_pairs, 0); // 2 < 10.
        assert!((agg.avg_coupling_strength - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn ownership_buckets() {
        let own = vec![
            FileOwnershipData { file: "a".into(), lines: 0, contributors: 1, top_contributor: String::new() },
            FileOwnershipData { file: "b".into(), lines: 0, contributors: 3, top_contributor: String::new() },
            FileOwnershipData { file: "c".into(), lines: 0, contributors: 9, top_contributor: String::new() },
        ];
        let buckets = bucket_ownership(&own);
        assert_eq!(buckets[0], OwnershipBucket { label: "Single owner".into(), count: 1 });
        assert_eq!(buckets[1].label, "2-3 owners");
        assert_eq!(buckets[1].count, 1);
        assert_eq!(buckets[3].label, "6+ owners");
        assert_eq!(buckets[3].count, 1);
    }

    #[test]
    fn filter_top_devs_keeps_within_limit() {
        let m = vec![row(&[(0, 5)]), row(&[(1, 3)])];
        let names = vec!["a".to_string(), "b".to_string()];
        let (fm, fn_) = filter_top_devs(&m, &names, 5);
        assert_eq!(fm.len(), 2);
        assert_eq!(fn_, names);
    }

    // --- DeveloperCoupling parity tests (ported from metrics_test.go) ---

    #[test]
    fn developer_coupling_single_pair_strength() {
        // Go: TestDeveloperCouplingMetric_SinglePair.
        // dev1 self=20, shared=10, dev2 self=15 → strength = 10 / avg(20,15) = 10/17.5.
        let input = ReportData {
            reversed_people_dict: vec!["Dev1|d1@x".into(), "Dev2|d2@x".into()],
            people_matrix: vec![row(&[(0, 20), (1, 10)]), row(&[(0, 10), (1, 15)])],
            ..Default::default()
        };
        let dc = compute_developer_coupling(&input);
        assert_eq!(dc.len(), 1);
        assert_eq!(dc[0].developer1, "Dev1");
        assert_eq!(dc[0].developer2, "Dev2");
        assert_eq!(dc[0].shared_files, 10);
        assert!((dc[0].strength - 10.0 / 17.5).abs() < 1e-9);
    }

    #[test]
    fn developer_coupling_multiple_pairs_sorted_by_shared() {
        // Go: TestDeveloperCouplingMetric_MultiplePairs_SortedBySharedFiles.
        let input = ReportData {
            reversed_people_dict: vec!["Dev1|".into(), "Dev2|".into(), "Dev3|".into()],
            people_matrix: vec![
                row(&[(0, 20), (1, 5), (2, 15)]),
                row(&[(0, 5), (1, 10), (2, 3)]),
                row(&[(0, 15), (1, 3), (2, 12)]),
            ],
            ..Default::default()
        };
        let dc = compute_developer_coupling(&input);
        assert_eq!(dc.len(), 3);
        assert_eq!(dc[0].shared_files, 15); // dev1-dev3.
        assert_eq!(dc[1].shared_files, 5); // dev1-dev2.
        assert_eq!(dc[2].shared_files, 3); // dev2-dev3.
    }

    #[test]
    fn developer_coupling_missing_dict_entry() {
        // Go: TestDeveloperCouplingMetric_MissingDictEntry. dev index 1 has no
        // dict entry → empty developer2 name/email.
        let input = ReportData {
            reversed_people_dict: vec!["Dev1|d1@x".into()],
            people_matrix: vec![row(&[(0, 20), (1, 10)]), row(&[(0, 10), (1, 15)])],
            ..Default::default()
        };
        let dc = compute_developer_coupling(&input);
        assert_eq!(dc.len(), 1);
        assert_eq!(dc[0].developer1, "Dev1");
        assert!(dc[0].developer2.is_empty());
    }

    #[test]
    fn developer_coupling_skips_zero_shared() {
        // Go: TestDeveloperCouplingMetric_SkipsZeroSharedChanges. No
        // off-diagonal entries → no pairs emitted.
        let input = ReportData {
            reversed_people_dict: vec!["Dev1|".into(), "Dev2|".into()],
            people_matrix: vec![row(&[(0, 20)]), row(&[(1, 15)])],
            ..Default::default()
        };
        assert!(compute_developer_coupling(&input).is_empty());
    }

    #[test]
    fn filter_top_devs_ranks_by_diagonal() {
        let m = vec![row(&[(0, 1)]), row(&[(1, 9)]), row(&[(2, 5)])];
        let names = vec!["a".into(), "b".into(), "c".into()];
        let (_fm, fn_) = filter_top_devs(&m, &names, 2);
        // top by diagonal: b(9), c(5).
        assert_eq!(fn_, vec!["b".to_string(), "c".to_string()]);
    }
}

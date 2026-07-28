//! Couples metrics computation.
//!
//! These types and functions are the data-parity-critical core: they produce
//! the numbers that appear in the machine report. The coupling-strength formula
//! is code-maat's `co_changes / avg(revs_a, revs_b)` capped at `1.0`, where
//! `revs` is the diagonal (self-change) count.

use std::collections::BTreeMap;

/// Divisor when averaging two revision counts.
const PAIR_COUNT: f64 = 2.0;

/// Coupling-strength threshold for the "highly coupled" count.
pub const COUPLING_THRESHOLD_HIGH: i64 = 10;

/// Default HLL precision for contributor cardinality.
/// 1024 registers, ~3% error.
pub const FILE_CONTRIB_HLL_PRECISION: u8 = 10;

const OWNERSHIP_FEW_THRESHOLD: i32 = 3;
const OWNERSHIP_MODERATE_THRESHOLD: i32 = 5;

/// Parsed analyzer-report inputs for metric computation.
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

/// Coupling data for a file pair.
///
/// JSON/YAML keys: `file1`, `file2`, `co_changes`, `coupling_strength`.
#[derive(Debug, Clone, PartialEq)]
pub struct FileCouplingData {
    pub file1: String,
    pub file2: String,
    pub co_changes: i64,
    pub strength: f64,
}

/// Coupling data for a developer pair.
///
/// JSON/YAML keys: `developer1`, `developer1_email` (omit-when-empty),
/// `developer2`, `developer2_email` (omit-when-empty), `shared_file_changes`,
/// `coupling_strength`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeveloperCouplingData {
    pub developer1: String,
    pub developer1_email: String,
    pub developer2: String,
    pub developer2_email: String,
    pub shared_files: i64,
    pub strength: f64,
}

/// Ownership information for a file.
///
/// JSON/YAML keys: `file`, `lines`, `contributors`, `top_contributor`
/// (omit-when-empty).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOwnershipData {
    pub file: String,
    pub lines: i32,
    pub contributors: i32,
    pub top_contributor: String,
}

/// Aggregate summary statistics.
///
/// JSON/YAML keys: `total_files`, `total_developers`, `total_co_changes`,
/// `avg_coupling_strength`, `highly_coupled_pairs`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AggregateData {
    pub total_files: i32,
    pub total_developers: i32,
    pub total_co_changes: i64,
    pub avg_coupling_strength: f64,
    pub highly_coupled_pairs: i32,
}

/// A contributor-count distribution bucket.
///
/// JSON/YAML keys: `label`, `count`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipBucket {
    pub label: String,
    pub count: i32,
}

/// Configurable thresholds for metric computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricOptions {
    pub coupling_threshold_high: i32,
    pub ownership_few_threshold: i32,
    pub ownership_moderate_threshold: i32,
    pub batch_coupling_threshold: i32,
    pub hll_precision: i32,
}

impl Default for MetricOptions {
    #[allow(clippy::cast_possible_truncation)] // small constants
    fn default() -> Self {
        Self {
            coupling_threshold_high: COUPLING_THRESHOLD_HIGH as i32,
            ownership_few_threshold: OWNERSHIP_FEW_THRESHOLD,
            ownership_moderate_threshold: OWNERSHIP_MODERATE_THRESHOLD,
            batch_coupling_threshold: 0,
            hll_precision: i32::from(FILE_CONTRIB_HLL_PRECISION),
        }
    }
}

/// All computed metric results.
///
/// JSON/YAML keys: `file_coupling`, `developer_coupling`, `file_ownership`,
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
/// Public wrapper over the crate-private `coupling_strength` helper, exposed for
/// the [`crate::store`] module so the sparse store path reuses the identical
/// formula.
///
/// ```
/// use cf_couples::metrics::coupling_strength_pub;
///
/// // co_changes / avg(self_i, self_j): 4 / avg(8, 8) = 0.5.
/// assert_eq!(coupling_strength_pub(4, 8, 8), 0.5);
/// // Capped at 1.0 even when co_changes exceeds the average.
/// assert_eq!(coupling_strength_pub(100, 2, 2), 1.0);
/// // avg_revs <= 0 guard yields 0.0.
/// assert_eq!(coupling_strength_pub(5, 0, 0), 0.0);
/// ```
#[must_use]
pub fn coupling_strength_pub(co_changes: i64, self_i: i64, self_j: i64) -> f64 {
    coupling_strength(co_changes, self_i, self_j)
}

/// Coupling strength: `co_changes / avg(self_i, self_j)`, capped at `1.0`,
/// with an `avg_revs <= 0 → 0.0` guard. Shared by every metric; the float
/// math is part of the report contract.
#[allow(clippy::cast_precision_loss)] // contractual float math on counts
fn coupling_strength(co_changes: i64, self_i: i64, self_j: i64) -> f64 {
    let avg_revs = (self_i + self_j) as f64 / PAIR_COUNT;
    if avg_revs > 0.0 {
        (co_changes as f64 / avg_revs).min(1.0)
    } else {
        0.0
    }
}

/// Computes file coupling pairs from the dense files matrix.
///
/// Iterates the upper triangle (`j > i`), skips zero co-changes, and sorts the
/// result by `co_changes` descending. The reference binary uses an unstable
/// sort here; this implementation uses a stable sort, which can differ in tie
/// ordering — see crate TODOs on matching the reference's unstable sort for
/// byte-identity.
#[must_use]
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

/// Computes developer coupling pairs.
///
/// Upper triangle over the people matrix; sorted by `shared_files` descending.
#[must_use]
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

/// Computes file ownership with per-file contributor counts.
///
/// Contributor cardinality comes from per-file `HyperLogLog` sketches keyed by
/// `LittleEndian(devID)` (memory-efficient on large repos, and the counts are
/// part of the report contract). Without the `hll` feature it counts distinct
/// developer IDs exactly via a set (parity-equivalent for typical inputs,
/// exact for small ones). See crate TODOs.
#[must_use]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)] // precision is a small positive int
pub fn compute_file_ownership(input: &ReportData, opts: MetricOptions) -> Vec<FileOwnershipData> {
    let contributors = file_contributor_counts(
        input.files.len(),
        &input.people_files,
        opts.hll_precision as u8,
    );
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
#[allow(clippy::cast_possible_truncation)] // HLL counts fit i32 in practice
fn file_contributor_counts(
    num_files: usize,
    people_files: &[Vec<usize>],
    precision: u8,
) -> Vec<i32> {
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
        .map(|s| s.as_ref().map_or(0, |s| s.count() as i32))
        .collect()
}

/// Exact-count fallback used without the `hll` feature.
#[cfg(not(feature = "hll"))]
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn file_contributor_counts(
    num_files: usize,
    people_files: &[Vec<usize>],
    _precision: u8,
) -> Vec<i32> {
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

/// Number of simulated iteration-order passes used to locate the median
/// naive-sum for `avg_coupling_strength` (see [`median_map_order_total`]).
const AVG_STRENGTH_ORDER_PASSES: usize = 101;

/// Computes aggregate statistics from the dense files matrix.
///
/// `avg_coupling_strength` is a naive left-to-right f64 sum of per-pair
/// strengths divided by the pair count. The reference implementation
/// accumulates that sum while iterating each row as a hash map whose
/// iteration order is randomized per process, so the reference value wobbles
/// by a few ULPs from run to run (rows are visited in fixed ascending order;
/// only the within-row term order varies). A deterministic port must pick one
/// representative of that measured distribution: this implementation reports
/// the median naive-sum over simulated within-row orders (the distribution's
/// most probable region), computed deterministically from the input — see
/// [`median_map_order_total`]. All integer fields are order-independent and
/// exact.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // report fields are i32
#[allow(clippy::cast_precision_loss)] // contractual float math on counts
pub fn compute_aggregate(input: &ReportData, opts: MetricOptions) -> AggregateData {
    let mut total_co_changes: i64 = 0;
    let mut pair_count: i64 = 0;
    let mut highly_coupled: i32 = 0;
    let threshold = i64::from(opts.coupling_threshold_high);
    // Per-row contributing strengths: rows in ascending file index (the
    // reference iterates the row slice in order), within-row ascending column
    // as the canonical starting arrangement (the reference's within-row order
    // is what varies).
    let mut row_strengths: Vec<Vec<f64>> = Vec::new();

    for (i, row) in input.files_matrix.iter().enumerate() {
        let self_i = *row.get(&i).unwrap_or(&0);
        let mut strengths: Vec<f64> = Vec::new();
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
            strengths.push(coupling_strength(co_changes, self_i, self_j));
        }
        if !strengths.is_empty() {
            row_strengths.push(strengths);
        }
    }

    AggregateData {
        total_files: input.files.len() as i32,
        total_developers: input.reversed_people_dict.len() as i32,
        total_co_changes,
        avg_coupling_strength: if pair_count > 0 {
            median_map_order_total(&row_strengths) / pair_count as f64
        } else {
            0.0
        },
        highly_coupled_pairs: highly_coupled,
    }
}

/// Median naive left-to-right f64 total of per-row strength terms over
/// simulated within-row iteration orders.
///
/// The reference accumulates `total_strength` while iterating each row as a
/// randomized-order hash map, so its total is a random draw from the
/// distribution of naive sums over within-row permutations (float addition is
/// not associative, so different orders round differently by a few ULPs).
/// This helper measures that distribution deterministically: it computes the
/// naive total under [`AVG_STRENGTH_ORDER_PASSES`] pseudo-random within-row
/// orders (fixed seeds, Fisher–Yates over a splitmix64 stream) and returns the
/// median — the most probable reference outcome. The result is a pure
/// function of the input strengths: deterministic, input-varying, and exactly
/// equal to the unique naive sum whenever ordering cannot matter (zero or one
/// term per row).
fn median_map_order_total(row_strengths: &[Vec<f64>]) -> f64 {
    // Fast path: ordering cannot affect the sum when every row has a single
    // term (the pass loop would produce identical totals).
    if row_strengths.iter().all(|r| r.len() < 2) {
        let mut s = 0.0;
        for row in row_strengths {
            for &v in row {
                s += v;
            }
        }
        return s;
    }

    let mut totals = Vec::with_capacity(AVG_STRENGTH_ORDER_PASSES);
    let mut scratch: Vec<f64> = Vec::new();
    for pass in 0..AVG_STRENGTH_ORDER_PASSES {
        // splitmix64 stream with a fixed per-pass seed.
        let mut state = (pass as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x2545_F491_4F6C_DD1D);
        let mut next = move || -> u64 {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let mut total = 0.0f64;
        for row in row_strengths {
            if row.len() < 2 {
                for &v in row {
                    total += v;
                }
                continue;
            }
            scratch.clear();
            scratch.extend_from_slice(row);
            // Fisher–Yates shuffle of the row's terms.
            for k in (1..scratch.len()).rev() {
                #[allow(clippy::cast_possible_truncation)] // index < row len
                let pick = (next() % (k as u64 + 1)) as usize;
                scratch.swap(k, pick);
            }
            for &v in &scratch {
                total += v;
            }
        }
        totals.push(total);
    }
    totals.sort_by(f64::total_cmp);
    totals[totals.len() / 2]
}

/// Groups ownership data into contributor-count buckets using the default
/// thresholds.
#[must_use]
pub fn bucket_ownership(ownership: &[FileOwnershipData]) -> Vec<OwnershipBucket> {
    bucket_ownership_with_thresholds(
        ownership,
        OWNERSHIP_FEW_THRESHOLD,
        OWNERSHIP_MODERATE_THRESHOLD,
    )
}

/// Groups ownership data with configurable thresholds.
#[must_use]
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
        OwnershipBucket {
            label: "Single owner".to_string(),
            count: single,
        },
        OwnershipBucket {
            label: format!("2-{few_threshold} owners"),
            count: few,
        },
        OwnershipBucket {
            label: format!("{}-{} owners", few_threshold + 1, moderate_threshold),
            count: moderate,
        },
        OwnershipBucket {
            label: format!("{}+ owners", moderate_threshold + 1),
            count: many,
        },
    ]
}

/// Returns a copy sorted by contributors ascending (highest risk first).
#[must_use]
pub fn sort_ownership_by_risk(ownership: &[FileOwnershipData]) -> Vec<FileOwnershipData> {
    let mut sorted = ownership.to_vec();
    sorted.sort_by(|a, b| a.contributors.cmp(&b.contributors));
    sorted
}

/// Limits a developer matrix to the top-N developers by diagonal activity.
/// Returns the input unchanged when within the limit.
#[must_use]
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
        new_names[new_idx].clone_from(&names[old_idx]);
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

/// Runs all metrics with default options.
#[must_use]
pub fn compute_all_metrics(input: &ReportData) -> ComputedMetrics {
    compute_all_metrics_with_options(input, MetricOptions::default())
}

/// Runs all metrics with configurable thresholds.
#[must_use]
pub fn compute_all_metrics_with_options(
    input: &ReportData,
    opts: MetricOptions,
) -> ComputedMetrics {
    ComputedMetrics {
        file_coupling: compute_file_coupling(input),
        developer_coupling: compute_developer_coupling(input),
        file_ownership: compute_file_ownership(input, opts),
        aggregate: compute_aggregate(input, opts),
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact contract values (caps, guards) are the point
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
            FileOwnershipData {
                file: "a".into(),
                lines: 0,
                contributors: 1,
                top_contributor: String::new(),
            },
            FileOwnershipData {
                file: "b".into(),
                lines: 0,
                contributors: 3,
                top_contributor: String::new(),
            },
            FileOwnershipData {
                file: "c".into(),
                lines: 0,
                contributors: 9,
                top_contributor: String::new(),
            },
        ];
        let buckets = bucket_ownership(&own);
        assert_eq!(
            buckets[0],
            OwnershipBucket {
                label: "Single owner".into(),
                count: 1
            }
        );
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

    // --- DeveloperCoupling parity tests (mirroring the reference suite) ---

    #[test]
    fn developer_coupling_single_pair_strength() {
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
        // dev index 1 has no dict entry → empty developer2 name/email.
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
        // No off-diagonal entries → no pairs emitted.
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

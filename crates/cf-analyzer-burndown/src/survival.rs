//! Survival arithmetic: the sparse → dense densification and every report
//! figure derived from the dense matrices (global, per file, per developer)
//! plus the developer interaction matrix.
//!
//! All inputs are pure data accumulated by the history walk; nothing here
//! reads the repository or a clock, so a report is a pure function of the
//! walk products and the sampling grid.

use std::collections::BTreeMap;

use crate::metrics::{
    AggregateData, ComputedMetrics, DeveloperSurvival, FileOwnership, FileSurvival, Interaction,
    SurvivalData,
};

/// Sparse history accumulated across the commit walk:
/// `curTick -> prevTick -> lineCountDelta` — how many lines written at
/// `prevTick` were added (positive) or deleted (negative) at `curTick`.
pub type SparseHistory = BTreeMap<i64, BTreeMap<i64, i64>>;

/// Dense burndown matrix: rows are samples, columns are age bands.
pub type DenseHistory = Vec<Vec<i64>>;

/// Hours per day, for the period figures.
const HOURS_PER_DAY: i64 = 24;

/// Percent scale of ownership shares.
const PERCENT: f64 = 100.0;

/// The sampling grid every dense matrix of one report is laid out on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    /// Ticks per sample row (`--sampling`).
    pub sampling: i64,
    /// Ticks per age band (`--granularity`).
    pub granularity: i64,
    /// The tick of the newest commit in the window; fixes the matrix size.
    pub last_tick: i64,
    /// `--tick-size` in hours.
    pub tick_size_hours: i64,
}

impl Grid {
    /// Number of sample rows.
    #[must_use]
    pub const fn samples(&self) -> usize {
        (self.last_tick / self.sampling + 1) as usize
    }

    /// Number of age bands.
    #[must_use]
    pub const fn bands(&self) -> usize {
        (self.last_tick / self.granularity + 1) as usize
    }

    /// Length of the analysis period in days: the span from the window's
    /// first tick to its last, not quantized to the sampling grid.
    #[must_use]
    pub const fn period_days(&self) -> i64 {
        self.last_tick * self.tick_size_hours / HOURS_PER_DAY
    }

    /// Densifies one sparse history on this grid.
    #[must_use]
    pub fn densify(&self, history: &SparseHistory) -> DenseHistory {
        group_sparse_history(history, self.sampling, self.granularity, self.last_tick)
    }
}

/// Densify a sparse tick history into a `samples x bands` matrix.
///
/// Sample `i` is the state after the last tick of the `i`-th sampling period
/// (`[i·sampling, (i+1)·sampling)`); band `j` holds the lines written in
/// ticks `[j·granularity, (j+1)·granularity)`. A sample with no recorded
/// tick repeats the previous row, so every row is a complete snapshot; a
/// `last_tick` beyond the newest recorded tick extends the carry-forward to
/// the end of the window.
///
/// ```
/// use cf_analyzer_burndown::{group_sparse_history, SparseHistory};
/// use std::collections::BTreeMap;
///
/// // One commit at tick 0 adds 100 lines authored at tick 0.
/// let mut history: SparseHistory = BTreeMap::new();
/// history.insert(0, BTreeMap::from([(0i64, 100i64)]));
///
/// // sampling = granularity = 10, last_tick = 0 → a 1x1 matrix.
/// let dense = group_sparse_history(&history, 10, 10, 0);
/// assert_eq!(dense, vec![vec![100]]);
///
/// // An empty history densifies to an empty matrix.
/// assert!(group_sparse_history(&SparseHistory::new(), 10, 10, 0).is_empty());
/// ```
#[must_use]
pub fn group_sparse_history(
    history: &SparseHistory,
    sampling: i64,
    granularity: i64,
    last_tick: i64,
) -> DenseHistory {
    if history.is_empty() {
        return Vec::new();
    }
    let grid = Grid {
        sampling,
        granularity,
        last_tick,
        tick_size_hours: HOURS_PER_DAY,
    };
    let (samples, bands) = (grid.samples(), grid.bands());
    let mut result: DenseHistory = vec![vec![0i64; bands]; samples];

    // Carry forward across empty sample rows, then add each tick's per-band
    // deltas into its sample row; the trailing carry fills up to last_tick.
    let mut prev_sample = 0usize;
    let mut carry_to = |result: &mut DenseHistory, sample: usize| {
        for row in prev_sample + 1..=sample.min(samples - 1) {
            let state = result[row - 1].clone();
            result[row].copy_from_slice(&state);
        }
        prev_sample = prev_sample.max(sample.min(samples - 1));
    };
    for (&tick, row) in history {
        let sample = (tick / sampling) as usize;
        if sample >= samples {
            break;
        }
        carry_to(&mut result, sample);
        for (&born, &delta) in row {
            let band = (born / granularity) as usize;
            if band < bands {
                result[sample][band] += delta;
            }
        }
    }
    carry_to(&mut result, samples - 1);
    result
}

/// Sum, over every band, of that band's maximum value across all samples —
/// the lines ever written that lived until a sample boundary.
#[must_use]
pub fn find_peak_lines(history: &DenseHistory) -> i64 {
    let Some(first) = history.first() else {
        return 0;
    };
    let mut band_peaks = vec![0i64; first.len()];
    for sample in history {
        for (peak, &v) in band_peaks.iter_mut().zip(sample) {
            *peak = (*peak).max(v);
        }
    }
    band_peaks.iter().sum()
}

/// Sum of the positive values in a band row.
fn sum_positive(values: &[i64]) -> i64 {
    values.iter().filter(|&&v| v > 0).sum()
}

/// Survival statistics for one sample row. A negative cell (a deletion of
/// lines no earlier sample ever credited, e.g. lines written and deleted
/// within one sampling period) counts as zero.
#[must_use]
pub fn compute_survival_sample(index: i64, sample: &[i64], peak_lines: i64) -> SurvivalData {
    let breakdown: Vec<i64> = sample.iter().map(|&v| v.max(0)).collect();
    let total: i64 = breakdown.iter().sum();
    SurvivalData {
        sample_index: index,
        total_lines: total,
        survival_rate: rate(total, peak_lines),
        band_breakdown: breakdown,
    }
}

/// `current / peak`, or 0 when nothing was ever written.
fn rate(current: i64, peak: i64) -> f64 {
    if peak > 0 {
        current as f64 / peak as f64
    } else {
        0.0
    }
}

/// The `(current, peak)` lines of one dense history.
fn current_and_peak(dense: &DenseHistory) -> (i64, i64) {
    let current = dense.last().map_or(0, |row| sum_positive(row));
    (current, find_peak_lines(dense))
}

/// The walk products one report is computed from.
pub struct MetricsInput<'a> {
    /// The global sparse history.
    pub global: &'a SparseHistory,
    /// Per-file sparse histories keyed by the file's final path; `None`
    /// when per-file tracking is off.
    pub files: Option<&'a BTreeMap<String, SparseHistory>>,
    /// Per-developer sparse histories keyed by person id; `None` when
    /// per-developer tracking is off.
    pub people: Option<&'a BTreeMap<i64, SparseHistory>>,
    /// Final line ownership per file (`path -> person -> lines`); only
    /// populated when both trackings are on.
    pub ownership: &'a BTreeMap<String, BTreeMap<i64, i64>>,
    /// The interaction matrix (`author -> modifier -> lines of the author
    /// deleted or replaced by the modifier`).
    pub matrix: &'a BTreeMap<i64, BTreeMap<i64, i64>>,
    /// Display name per person id (index = id).
    pub names: &'a [String],
    /// The sampling grid.
    pub grid: Grid,
}

/// Display name of a person id, `dev_<id>` past the end of the dictionary.
fn person_name(id: i64, names: &[String]) -> String {
    usize::try_from(id)
        .ok()
        .and_then(|i| names.get(i))
        .cloned()
        .unwrap_or_else(|| format!("dev_{id}"))
}

/// Computes the whole burndown report from the walk products.
///
/// ```
/// use cf_analyzer_burndown::{compute_metrics, Grid, MetricsInput, SparseHistory};
/// use std::collections::BTreeMap;
///
/// let mut global: SparseHistory = BTreeMap::new();
/// global.insert(0, BTreeMap::from([(0, 10)]));
/// global.insert(31, BTreeMap::from([(0, -3), (31, 5)]));
/// let empty = BTreeMap::new();
/// let m = compute_metrics(&MetricsInput {
///     global: &global,
///     files: None,
///     people: None,
///     ownership: &empty,
///     matrix: &BTreeMap::new(),
///     names: &[],
///     grid: Grid { sampling: 30, granularity: 30, last_tick: 31, tick_size_hours: 24 },
/// });
/// assert_eq!(m.aggregate.total_current_lines, 12);
/// assert_eq!(m.aggregate.total_peak_lines, 15);
/// assert_eq!(m.aggregate.analysis_period_days, 31);
/// assert_eq!(m.global_survival[1].band_breakdown, vec![7, 5]);
/// ```
#[must_use]
pub fn compute_metrics(input: &MetricsInput<'_>) -> ComputedMetrics {
    let grid = input.grid;
    let global_dense = grid.densify(input.global);
    let (current, peak) = current_and_peak(&global_dense);
    let global_survival: Vec<SurvivalData> = global_dense
        .iter()
        .enumerate()
        .map(|(i, sample)| compute_survival_sample(i as i64, sample, peak))
        .collect();

    let file_survival = input
        .files
        .map(|files| file_survival(files, input, grid))
        .unwrap_or_default();
    let developer_survival = input
        .people
        .map(|people| developer_survival(people, input.names, grid))
        .unwrap_or_default();
    let interactions = if input.people.is_some() {
        interactions(input.matrix, input.names)
    } else {
        Vec::new()
    };

    let aggregate = AggregateData {
        total_current_lines: current,
        total_peak_lines: peak,
        overall_survival_rate: rate(current, peak),
        analysis_period_days: if global_dense.is_empty() {
            0
        } else {
            grid.period_days()
        },
        num_bands: global_dense.first().map_or(0, |r| r.len() as i64),
        num_samples: global_dense.len() as i64,
        tracked_files: file_survival.len() as i64,
        tracked_developers: developer_survival.len() as i64,
    };
    ComputedMetrics {
        aggregate,
        global_survival,
        file_survival,
        developer_survival,
        interactions,
    }
}

/// Per-file survival, most surviving lines first (path breaks ties).
fn file_survival(
    files: &BTreeMap<String, SparseHistory>,
    input: &MetricsInput<'_>,
    grid: Grid,
) -> Vec<FileSurvival> {
    let mut out: Vec<FileSurvival> = files
        .iter()
        .map(|(path, history)| {
            let (current, peak) = current_and_peak(&grid.densify(history));
            FileSurvival {
                path: path.clone(),
                current_lines: current,
                peak_lines: peak,
                survival_rate: rate(current, peak),
                ownership: input.people.map(|_| {
                    file_ownership(
                        input.ownership.get(path).unwrap_or(&BTreeMap::new()),
                        input.names,
                    )
                }),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.current_lines
            .cmp(&a.current_lines)
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

/// The ownership of one file: the live lines per person and the top owner
/// (most lines; the smaller id on a tie).
fn file_ownership(lines_by_person: &BTreeMap<i64, i64>, names: &[String]) -> FileOwnership {
    let total: i64 = lines_by_person.values().sum();
    let top = lines_by_person
        .iter()
        .filter(|(_, &lines)| lines > 0)
        .max_by(|(ida, la), (idb, lb)| la.cmp(lb).then_with(|| idb.cmp(ida)))
        .map(|(&id, &lines)| {
            (
                id,
                person_name(id, names),
                lines as f64 / total as f64 * PERCENT,
            )
        });
    FileOwnership {
        lines_by_person: lines_by_person
            .iter()
            .filter(|(_, &lines)| lines > 0)
            .map(|(&id, &lines)| (id, lines))
            .collect(),
        top_owner: top,
    }
}

/// Per-developer survival for EVERY name in the dictionary (a developer
/// with no surviving history reports zeros), most surviving lines first (id
/// breaks ties).
fn developer_survival(
    people: &BTreeMap<i64, SparseHistory>,
    names: &[String],
    grid: Grid,
) -> Vec<DeveloperSurvival> {
    let empty = SparseHistory::new();
    let mut out: Vec<DeveloperSurvival> = (0..names.len() as i64)
        .map(|id| {
            let history = people.get(&id).unwrap_or(&empty);
            let (current, peak) = current_and_peak(&grid.densify(history));
            DeveloperSurvival {
                id,
                name: person_name(id, names),
                current_lines: current,
                peak_lines: peak,
                survival_rate: rate(current, peak),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.current_lines
            .cmp(&a.current_lines)
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// The interaction list: one entry per `(author, modifier)` pair with lines
/// of the author deleted or replaced by the modifier, most lines first
/// (author id, then modifier id, break ties).
fn interactions(matrix: &BTreeMap<i64, BTreeMap<i64, i64>>, names: &[String]) -> Vec<Interaction> {
    let mut out: Vec<Interaction> = matrix
        .iter()
        .flat_map(|(&author, row)| {
            row.iter()
                .filter(|(_, &lines)| lines > 0)
                .map(move |(&modifier, &lines)| Interaction {
                    author_id: author,
                    author_name: person_name(author, names),
                    modifier_id: modifier,
                    modifier_name: person_name(modifier, names),
                    lines_modified: lines,
                    is_self_modify: author == modifier,
                })
        })
        .collect();
    out.sort_by(|a, b| {
        b.lines_modified
            .cmp(&a.lines_modified)
            .then_with(|| a.author_id.cmp(&b.author_id))
            .then_with(|| a.modifier_id.cmp(&b.modifier_id))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sparse history from `(cur, prev, delta)` triples.
    fn sparse(cells: &[(i64, i64, i64)]) -> SparseHistory {
        let mut h = SparseHistory::new();
        for &(cur, prev, delta) in cells {
            *h.entry(cur).or_default().entry(prev).or_default() += delta;
        }
        h
    }

    const DAILY: Grid = Grid {
        sampling: 30,
        granularity: 30,
        last_tick: 310,
        tick_size_hours: 24,
    };

    fn input<'a>(
        global: &'a SparseHistory,
        files: Option<&'a BTreeMap<String, SparseHistory>>,
        people: Option<&'a BTreeMap<i64, SparseHistory>>,
        ownership: &'a BTreeMap<String, BTreeMap<i64, i64>>,
        matrix: &'a BTreeMap<i64, BTreeMap<i64, i64>>,
        names: &'a [String],
        grid: Grid,
    ) -> MetricsInput<'a> {
        MetricsInput {
            global,
            files,
            people,
            ownership,
            matrix,
            names,
            grid,
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    // ---- group_sparse_history --------------------------------------------

    #[test]
    fn empty_history_densifies_to_nothing() {
        assert!(group_sparse_history(&SparseHistory::new(), 30, 30, 310).is_empty());
    }

    #[test]
    fn carry_forward_fills_empty_samples_between_recorded_ticks() {
        let h = sparse(&[(0, 0, 10), (95, 95, 4)]);
        let dense = group_sparse_history(&h, 30, 30, 95);
        assert_eq!(
            dense,
            vec![
                vec![10, 0, 0, 0],
                vec![10, 0, 0, 0],
                vec![10, 0, 0, 0],
                vec![10, 0, 0, 4],
            ]
        );
    }

    #[test]
    fn several_ticks_in_one_sample_accumulate_into_that_row() {
        let h = sparse(&[(0, 0, 10), (5, 5, 2), (12, 0, -3), (29, 29, 1)]);
        let dense = group_sparse_history(&h, 30, 30, 29);
        assert_eq!(dense, vec![vec![10]]);
        let fine = group_sparse_history(&h, 10, 10, 29);
        assert_eq!(fine, vec![vec![12, 0, 0], vec![9, 0, 0], vec![9, 0, 1]]);
    }

    #[test]
    fn last_tick_beyond_the_newest_key_extends_the_carry_forward() {
        let h = sparse(&[(0, 0, 10), (31, 0, -3), (31, 31, 5)]);
        let dense = group_sparse_history(&h, 30, 30, 125);
        assert_eq!(dense.len(), 5);
        assert_eq!(dense[0], vec![10, 0, 0, 0, 0]);
        for row in &dense[1..] {
            assert_eq!(*row, vec![7, 5, 0, 0, 0]);
        }
    }

    #[test]
    fn ticks_past_the_grid_are_ignored_not_panicked_on() {
        let h = sparse(&[(0, 0, 10), (400, 400, 1)]);
        let dense = group_sparse_history(&h, 30, 30, 59);
        assert_eq!(dense, vec![vec![10, 0], vec![10, 0]]);
    }

    #[test]
    fn sampling_and_granularity_are_independent() {
        let h = sparse(&[(0, 0, 10), (31, 31, 5), (62, 62, 1)]);
        let dense = group_sparse_history(&h, 60, 20, 62);
        // 2 samples (0..59, 60..119), 4 bands of 20 ticks.
        assert_eq!(dense, vec![vec![10, 5, 0, 0], vec![10, 5, 0, 1]]);
    }

    // ---- find_peak_lines / compute_survival_sample ---------------------------

    #[test]
    fn peak_is_the_sum_of_per_band_maxima() {
        let dense = vec![vec![10, 0, 0], vec![7, 5, 0], vec![0, 5, 6]];
        assert_eq!(find_peak_lines(&dense), 10 + 5 + 6);
        assert_eq!(find_peak_lines(&Vec::new()), 0);
    }

    #[test]
    fn survival_sample_suppresses_negative_cells() {
        let s = compute_survival_sample(3, &[7, -2, 5], 20);
        assert_eq!(s.sample_index, 3);
        assert_eq!(s.band_breakdown, vec![7, 0, 5]);
        assert_eq!(s.total_lines, 12);
        assert!((s.survival_rate - 0.6).abs() < 1e-12);
        assert_eq!(compute_survival_sample(0, &[1], 0).survival_rate, 0.0);
    }

    // ---- compute_metrics ----------------------------------------------------

    #[test]
    fn empty_walk_yields_an_empty_report() {
        let empty = BTreeMap::new();
        let m = compute_metrics(&input(
            &SparseHistory::new(),
            None,
            None,
            &empty,
            &BTreeMap::new(),
            &[],
            DAILY,
        ));
        assert_eq!(m.aggregate, AggregateData::default());
        assert!(m.global_survival.is_empty());
        assert!(m.file_survival.is_empty());
        assert!(m.developer_survival.is_empty());
        assert!(m.interactions.is_empty());
    }

    #[test]
    fn period_days_is_the_tick_span_not_the_sampling_grid() {
        let global = sparse(&[(0, 0, 10)]);
        let empty = BTreeMap::new();
        for (last_tick, hours, want) in [(310, 24, 310), (10, 24, 10), (48, 1, 2), (0, 24, 0)] {
            let grid = Grid {
                last_tick,
                tick_size_hours: hours,
                ..DAILY
            };
            let m = compute_metrics(&input(
                &global,
                None,
                None,
                &empty,
                &BTreeMap::new(),
                &[],
                grid,
            ));
            assert_eq!(m.aggregate.analysis_period_days, want, "{grid:?}");
            assert_eq!(m.aggregate.num_samples, last_tick / 30 + 1);
        }
    }

    #[test]
    fn global_survival_never_exceeds_one_and_bands_sum_to_totals() {
        let global = sparse(&[
            (0, 0, 10),
            (31, 0, -3),
            (31, 31, 5),
            (93, 93, 4),
            (186, 0, -7),
            (186, 31, -5),
            (186, 186, 6),
        ]);
        let empty = BTreeMap::new();
        let m = compute_metrics(&input(
            &global,
            None,
            None,
            &empty,
            &BTreeMap::new(),
            &[],
            DAILY,
        ));
        assert_eq!(m.aggregate.total_peak_lines, 10 + 5 + 4 + 6);
        assert_eq!(m.aggregate.total_current_lines, 10);
        for s in &m.global_survival {
            assert!(s.survival_rate <= 1.0);
            assert_eq!(s.band_breakdown.iter().sum::<i64>(), s.total_lines);
        }
        assert_eq!(m.aggregate.tracked_files, 0);
        assert_eq!(m.aggregate.tracked_developers, 0);
    }

    #[test]
    fn file_survival_is_sorted_and_carries_ownership_only_with_people() {
        let global = sparse(&[(0, 0, 10)]);
        let mut files = BTreeMap::new();
        files.insert("b.go".to_string(), sparse(&[(0, 0, 10), (31, 0, -10)]));
        files.insert(
            "script.py".to_string(),
            sparse(&[(93, 93, 4), (124, 124, 4)]),
        );
        files.insert("a.go".to_string(), sparse(&[(0, 0, 8)]));
        let mut ownership = BTreeMap::new();
        ownership.insert("script.py".to_string(), BTreeMap::from([(0, 4), (1, 4)]));
        let names = names(&["alice", "bob"]);

        let m = compute_metrics(&input(
            &global,
            Some(&files),
            None,
            &ownership,
            &BTreeMap::new(),
            &names,
            DAILY,
        ));
        let order: Vec<(&str, i64, i64)> = m
            .file_survival
            .iter()
            .map(|f| (f.path.as_str(), f.current_lines, f.peak_lines))
            .collect();
        assert_eq!(
            order,
            vec![("a.go", 8, 8), ("script.py", 8, 8), ("b.go", 0, 10)],
            "surviving lines desc, path asc on a tie"
        );
        assert!(m.file_survival.iter().all(|f| f.ownership.is_none()));
        assert_eq!(m.aggregate.tracked_files, 3);

        let people = BTreeMap::new();
        let m = compute_metrics(&input(
            &global,
            Some(&files),
            Some(&people),
            &ownership,
            &BTreeMap::new(),
            &names,
            DAILY,
        ));
        let script = m
            .file_survival
            .iter()
            .find(|f| f.path == "script.py")
            .expect("script.py");
        let own = script.ownership.as_ref().expect("ownership with people");
        assert_eq!(own.lines_by_person, BTreeMap::from([(0, 4), (1, 4)]));
        let (id, name, pct) = own.top_owner.clone().expect("top owner");
        assert_eq!((id, name.as_str()), (0, "alice"), "tie → smaller id");
        assert!((pct - 50.0).abs() < 1e-12);
        let dead = m.file_survival.iter().find(|f| f.path == "b.go").unwrap();
        assert!(dead.ownership.as_ref().unwrap().top_owner.is_none());
    }

    #[test]
    fn developer_survival_lists_every_name_sorted_by_surviving_lines() {
        let global = sparse(&[(0, 0, 10)]);
        let mut people = BTreeMap::new();
        people.insert(
            0,
            sparse(&[(0, 0, 10), (31, 0, -3), (186, 0, -7), (186, 186, 6)]),
        );
        people.insert(1, sparse(&[(31, 31, 5), (124, 124, 3)]));
        let names = names(&["alice", "bob", "carol"]);
        let empty = BTreeMap::new();
        let m = compute_metrics(&input(
            &global,
            None,
            Some(&people),
            &empty,
            &BTreeMap::new(),
            &names,
            DAILY,
        ));
        let rows: Vec<(i64, &str, i64, i64)> = m
            .developer_survival
            .iter()
            .map(|d| (d.id, d.name.as_str(), d.current_lines, d.peak_lines))
            .collect();
        assert_eq!(
            rows,
            vec![(1, "bob", 8, 8), (0, "alice", 6, 16), (2, "carol", 0, 0)]
        );
        assert_eq!(m.aggregate.tracked_developers, 3);
        assert!(m.file_survival.is_empty());
    }

    #[test]
    fn interactions_are_the_positive_matrix_cells_sorted_by_lines() {
        let global = sparse(&[(0, 0, 10)]);
        let people = BTreeMap::new();
        let mut matrix = BTreeMap::new();
        matrix.insert(0, BTreeMap::from([(0, 7), (1, 4)]));
        matrix.insert(1, BTreeMap::from([(0, 5), (1, 0)]));
        let names = names(&["alice", "bob"]);
        let empty = BTreeMap::new();
        let m = compute_metrics(&input(
            &global,
            None,
            Some(&people),
            &empty,
            &matrix,
            &names,
            DAILY,
        ));
        let rows: Vec<(&str, &str, i64, bool)> = m
            .interactions
            .iter()
            .map(|i| {
                (
                    i.author_name.as_str(),
                    i.modifier_name.as_str(),
                    i.lines_modified,
                    i.is_self_modify,
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                ("alice", "alice", 7, true),
                ("bob", "alice", 5, false),
                ("alice", "bob", 4, false),
            ]
        );
        // Without people tracking the matrix is ignored.
        let m = compute_metrics(&input(&global, None, None, &empty, &matrix, &names, DAILY));
        assert!(m.interactions.is_empty());
    }

    #[test]
    fn unknown_ids_get_a_placeholder_name() {
        assert_eq!(person_name(7, &names(&["a"])), "dev_7");
        assert_eq!(person_name(-1, &names(&["a"])), "dev_-1");
        assert_eq!(person_name(0, &names(&["a"])), "a");
    }
}

//! Burndown `ComputedMetrics` model and byte-identical serialization.
//!
//! Mirrors Go `internal/analyzers/burndown/metrics.go`: the per-analyzer
//! `ComputedMetrics` struct that `BaseHistoryAnalyzer.Serialize` marshals for the
//! `json` / `yaml` / `bin` machine formats. Field/key order follows the Go struct
//! declaration order (`aggregate`, `global_survival`, `file_survival`,
//! `developer_survival`, `interactions`), which the struct-origin [`GoMap`]
//! preserves.
//!
//! Go nil-slice asymmetry: `encoding/json` renders a nil slice as `null`, while
//! `gopkg.in/yaml.v3` renders both nil and empty slices as `[]`. The two builders
//! [`ComputedMetrics::to_go_value`] (JSON/bin) and
//! [`ComputedMetrics::to_go_value_yaml`] (YAML) differ only on this point.

use cf_gojson::{GoMap, GoValue};

/// Code-survival statistics for one sample (Go `SurvivalData`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurvivalData {
    /// `sample_index`.
    pub sample_index: i64,
    /// `total_lines`.
    pub total_lines: i64,
    /// `survival_rate`.
    pub survival_rate: f64,
    /// `band_breakdown`.
    pub band_breakdown: Vec<i64>,
}

impl SurvivalData {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("sample_index", GoValue::Int(self.sample_index));
        m.push("total_lines", GoValue::Int(self.total_lines));
        m.push("survival_rate", GoValue::Float(self.survival_rate));
        m.push(
            "band_breakdown",
            GoValue::Array(self.band_breakdown.iter().map(|&v| GoValue::Int(v)).collect()),
        );
        GoValue::Map(m)
    }
}

/// Summary statistics (Go `AggregateData`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AggregateData {
    /// `total_current_lines`.
    pub total_current_lines: i64,
    /// `total_peak_lines`.
    pub total_peak_lines: i64,
    /// `overall_survival_rate`.
    pub overall_survival_rate: f64,
    /// `analysis_period_days`.
    pub analysis_period_days: i64,
    /// `num_bands`.
    pub num_bands: i64,
    /// `num_samples`.
    pub num_samples: i64,
    /// `tracked_files`.
    pub tracked_files: i64,
    /// `tracked_developers`.
    pub tracked_developers: i64,
}

impl AggregateData {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("total_current_lines", GoValue::Int(self.total_current_lines));
        m.push("total_peak_lines", GoValue::Int(self.total_peak_lines));
        m.push("overall_survival_rate", GoValue::Float(self.overall_survival_rate));
        m.push("analysis_period_days", GoValue::Int(self.analysis_period_days));
        m.push("num_bands", GoValue::Int(self.num_bands));
        m.push("num_samples", GoValue::Int(self.num_samples));
        m.push("tracked_files", GoValue::Int(self.tracked_files));
        m.push("tracked_developers", GoValue::Int(self.tracked_developers));
        GoValue::Map(m)
    }
}

/// All computed burndown metric results (Go `ComputedMetrics`).
///
/// `file_survival`, `developer_survival`, and `interactions` are modeled as
/// optional vectors; `None` reproduces a Go nil slice (`null` in JSON, `[]` in
/// YAML), `Some` an explicit (possibly empty) slice (`[]` in both).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputedMetrics {
    /// `aggregate`.
    pub aggregate: AggregateData,
    /// `global_survival`.
    pub global_survival: Vec<SurvivalData>,
    /// `file_survival`: `None` = Go nil slice.
    pub file_survival: Option<Vec<GoValue>>,
    /// `developer_survival`: `None` = Go nil slice.
    pub developer_survival: Option<Vec<GoValue>>,
    /// `interactions`: `None` = Go nil slice.
    pub interactions: Option<Vec<GoValue>>,
}

impl ComputedMetrics {
    fn build(&self, nil_as_empty: bool) -> GoValue {
        let nil = |opt: &Option<Vec<GoValue>>| -> GoValue {
            match opt {
                Some(v) => GoValue::Array(v.clone()),
                None => {
                    if nil_as_empty {
                        GoValue::Array(Vec::new())
                    } else {
                        GoValue::Null
                    }
                }
            }
        };

        let mut m = GoMap::new_struct();
        m.push("aggregate", self.aggregate.to_go_value());
        m.push(
            "global_survival",
            GoValue::Array(self.global_survival.iter().map(SurvivalData::to_go_value).collect()),
        );
        m.push("file_survival", nil(&self.file_survival));
        m.push("developer_survival", nil(&self.developer_survival));
        m.push("interactions", nil(&self.interactions));
        GoValue::Map(m)
    }

    /// Build the JSON/bin value tree (Go `encoding/json`: nil slice → `null`).
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        self.build(false)
    }

    /// Build the YAML value tree (`gopkg.in/yaml.v3`: nil slice → `[]`).
    #[must_use]
    pub fn to_go_value_yaml(&self) -> GoValue {
        self.build(true)
    }
}

/// Sparse global history accumulated across the commit walk:
/// `curTick -> prevTick -> lineCountDelta`. Mirrors the Go `sparseHistory`
/// (`map[int]map[int]int64`) that the burndown aggregator builds by additively
/// merging every per-commit `CommitResult.GlobalDeltas`.
pub type SparseHistory = std::collections::BTreeMap<i64, std::collections::BTreeMap<i64, i64>>;

/// Dense burndown matrix (Go `DenseHistory = [][]int64`): rows are samples,
/// columns are cohort bands.
pub type DenseHistory = Vec<Vec<i64>>;

/// `groupSparseHistory` (Go `internal/analyzers/burndown/history_dense.go`):
/// densify a sparse tick history into a `samples x bands` matrix. `sampling` and
/// `granularity` are the band/sample sizes (both 30 by default); `last_tick` is
/// the maximum tick observed across the whole walk.
#[must_use]
pub fn group_sparse_history(history: &SparseHistory, sampling: i64, granularity: i64, last_tick: i64) -> DenseHistory {
    if history.is_empty() {
        return Vec::new();
    }

    // normalizeTicks: sorted tick keys (BTreeMap already sorted); append
    // last_tick if it exceeds the largest key (last_tick >= 0 here).
    let mut ticks: Vec<i64> = history.keys().copied().collect();
    let resolved_last = if let Some(&max_key) = ticks.last() {
        if max_key < last_tick {
            ticks.push(last_tick);
            last_tick
        } else {
            last_tick
        }
    } else {
        last_tick.max(0)
    };

    let samples = (resolved_last / sampling + 1) as usize;
    let bands = (resolved_last / granularity + 1) as usize;

    let mut result: DenseHistory = vec![vec![0i64; bands]; samples];

    // fillDenseHistory: carry forward state across empty sample rows, then add
    // each tick's per-band deltas into its sample row.
    let mut prevsi: usize = 0;
    for &tick in &ticks {
        let si = (tick / sampling) as usize;
        if si > prevsi {
            let state = result[prevsi].clone();
            for row in result.iter_mut().take(si + 1).skip(prevsi + 1) {
                row.copy_from_slice(&state);
            }
            prevsi = si;
        }
        if let Some(row) = history.get(&tick) {
            for (&t, &value) in row {
                let band = (t / granularity) as usize;
                if band < bands {
                    result[si][band] += value;
                }
            }
        }
    }

    result
}

/// `findPeakLines` (Go `metrics.go`): sum, over every band, of that band's
/// maximum value across all samples — the total lines ever written.
fn find_peak_lines(history: &DenseHistory) -> i64 {
    if history.is_empty() {
        return 0;
    }
    let num_bands = history[0].len();
    let mut band_peaks = vec![0i64; num_bands];
    for sample in history {
        for band in 0..sample.len().min(num_bands) {
            if sample[band] > band_peaks[band] {
                band_peaks[band] = sample[band];
            }
        }
    }
    band_peaks.iter().sum()
}

/// `sumPositiveValues` (Go `metrics.go`).
fn sum_positive(values: &[i64]) -> i64 {
    values.iter().filter(|&&v| v > 0).sum()
}

/// `computeSurvivalSample` (Go `metrics.go`).
fn compute_survival_sample(index: i64, sample: &[i64], peak_lines: i64) -> SurvivalData {
    let mut breakdown = vec![0i64; sample.len()];
    let mut total = 0i64;
    for (j, &v) in sample.iter().enumerate() {
        if v > 0 {
            total += v;
            breakdown[j] = v;
        }
    }
    let survival_rate = if peak_lines > 0 { total as f64 / peak_lines as f64 } else { 0.0 };
    SurvivalData { sample_index: index, total_lines: total, survival_rate, band_breakdown: breakdown }
}

/// `ComputeAllMetrics` (Go `metrics.go`) for the default report shape
/// (`PeopleNumber == 0`, `TrackFiles == false`): only `GlobalHistory`,
/// `Sampling`, `Granularity`, and `TickSize` feed the output, so
/// `file_survival` / `developer_survival` stay empty and `interactions` nil.
///
/// `tick_size_hours` is the configured tick size in hours (24 by default);
/// `analysis_period_days = (num_samples-1) * sampling * tick_size_hours / 24`,
/// matching Go's `time.Duration` arithmetic in `computeAggregate`.
#[must_use]
pub fn compute_global_metrics(global_dense: &DenseHistory, sampling: i64, tick_size_hours: i64) -> ComputedMetrics {
    // computeGlobalSurvival.
    let peak = find_peak_lines(global_dense);
    let global_survival: Vec<SurvivalData> = global_dense
        .iter()
        .enumerate()
        .map(|(i, sample)| compute_survival_sample(i as i64, sample, peak))
        .collect();

    // computeAggregate (TrackedFiles/TrackedDevelopers are 0: no per-file /
    // per-people histories in the default report).
    let mut aggregate = AggregateData::default();
    if !global_dense.is_empty() {
        let num_samples = global_dense.len() as i64;
        aggregate.num_samples = num_samples;
        aggregate.num_bands = global_dense[0].len() as i64;
        let total_ticks = (num_samples - 1) * sampling;
        // time.Duration(totalTicks) * tickSize / 24h, integer-truncated.
        aggregate.analysis_period_days = total_ticks * tick_size_hours / 24;
        aggregate.total_peak_lines = peak;
        aggregate.total_current_lines = sum_positive(&global_dense[num_samples as usize - 1]);
        if aggregate.total_peak_lines > 0 {
            aggregate.overall_survival_rate =
                aggregate.total_current_lines as f64 / aggregate.total_peak_lines as f64;
        }
    }

    ComputedMetrics {
        aggregate,
        global_survival,
        // computeFileSurvival over an empty FileOwnership map → empty slice
        // (make([]FileSurvivalData, 0, ...)); same for developer_survival.
        file_survival: Some(Vec::new()),
        developer_survival: Some(Vec::new()),
        // computeInteraction over an empty PeopleMatrix → nil slice.
        interactions: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head_metrics(n: i64) -> ComputedMetrics {
        ComputedMetrics {
            aggregate: AggregateData {
                total_current_lines: n,
                total_peak_lines: n,
                overall_survival_rate: 1.0,
                analysis_period_days: 0,
                num_bands: 1,
                num_samples: 1,
                tracked_files: 0,
                tracked_developers: 0,
            },
            global_survival: vec![SurvivalData {
                sample_index: 0,
                total_lines: n,
                survival_rate: 1.0,
                band_breakdown: vec![n],
            }],
            file_survival: Some(Vec::new()),
            developer_survival: Some(Vec::new()),
            interactions: None,
        }
    }

    #[test]
    fn json_matches_golden_shape() {
        let bytes = cf_gojson::marshal(&head_metrics(112_539).to_go_value());
        let s = String::from_utf8(bytes).unwrap();
        assert_eq!(
            s,
            r#"{"aggregate":{"total_current_lines":112539,"total_peak_lines":112539,"overall_survival_rate":1,"analysis_period_days":0,"num_bands":1,"num_samples":1,"tracked_files":0,"tracked_developers":0},"global_survival":[{"sample_index":0,"total_lines":112539,"survival_rate":1,"band_breakdown":[112539]}],"file_survival":[],"developer_survival":[],"interactions":null}"#
        );
    }

    #[test]
    fn yaml_renders_nil_slice_as_empty() {
        let bytes = cf_goyaml::marshal(&head_metrics(112_539).to_go_value_yaml());
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("interactions: []"), "{s}");
        assert!(s.contains("file_survival: []"), "{s}");
        assert!(s.contains("    total_current_lines: 112539"), "{s}");
        assert!(s.contains("        - 112539"), "{s}");
    }
}

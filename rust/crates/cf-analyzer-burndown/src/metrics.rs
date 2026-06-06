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

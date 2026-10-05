//! The burndown report model and its serialization.
//!
//! [`ComputedMetrics`] is the value every machine format encodes; the
//! json / bin / yaml encoders all take the same [`ComputedMetrics::to_go_value`]
//! tree, so the formats agree field for field (an empty list is `[]`
//! everywhere). Field order follows the declaration order (`aggregate`,
//! `global_survival`, `file_survival`, `developer_survival`,
//! `interactions`), which the struct-origin [`GoMap`] preserves.
//!
//! The figures are computed by [`crate::survival::compute_metrics`].

use std::collections::BTreeMap;

use cf_gojson::{GoMap, GoValue};

/// Code-survival statistics for one sample.
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
        m.push("band_breakdown", int_array(&self.band_breakdown));
        GoValue::Map(m)
    }
}

/// Summary statistics.
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
        m.push(
            "total_current_lines",
            GoValue::Int(self.total_current_lines),
        );
        m.push("total_peak_lines", GoValue::Int(self.total_peak_lines));
        m.push(
            "overall_survival_rate",
            GoValue::Float(self.overall_survival_rate),
        );
        m.push(
            "analysis_period_days",
            GoValue::Int(self.analysis_period_days),
        );
        m.push("num_bands", GoValue::Int(self.num_bands));
        m.push("num_samples", GoValue::Int(self.num_samples));
        m.push("tracked_files", GoValue::Int(self.tracked_files));
        m.push("tracked_developers", GoValue::Int(self.tracked_developers));
        GoValue::Map(m)
    }
}

/// Who owns the live lines of one file at the end of the window
/// (`--burndown-files` with `--burndown-people`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileOwnership {
    /// Live lines per person id (persons with zero lines are left out).
    pub lines_by_person: BTreeMap<i64, i64>,
    /// `(id, name, percentage of the file's live lines)` of the person with
    /// the most lines (the smaller id on a tie); `None` for a file with no
    /// live lines.
    pub top_owner: Option<(i64, String, f64)>,
}

/// Survival of one file (`--burndown-files`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileSurvival {
    /// The file's path at the end of the window (renames followed).
    pub path: String,
    /// Live lines at the last sample.
    pub current_lines: i64,
    /// Lines ever written to the file that lived until a sample boundary.
    pub peak_lines: i64,
    /// `current_lines / peak_lines`.
    pub survival_rate: f64,
    /// Ownership; `None` without `--burndown-people`.
    pub ownership: Option<FileOwnership>,
}

impl FileSurvival {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("path", GoValue::Str(self.path.clone()));
        m.push("current_lines", GoValue::Int(self.current_lines));
        m.push("peak_lines", GoValue::Int(self.peak_lines));
        m.push("survival_rate", GoValue::Float(self.survival_rate));
        if let Some(own) = &self.ownership {
            let mut lines = GoMap::new_int_map();
            for (id, n) in &own.lines_by_person {
                lines.insert(id.to_string(), GoValue::Int(*n));
            }
            m.push("ownership", GoValue::Map(lines));
            if let Some((id, name, pct)) = &own.top_owner {
                m.push("top_owner_id", GoValue::Int(*id));
                m.push("top_owner_name", GoValue::Str(name.clone()));
                m.push("top_owner_percentage", GoValue::Float(*pct));
            }
        }
        GoValue::Map(m)
    }
}

/// Survival of the lines one developer wrote (`--burndown-people`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeveloperSurvival {
    /// Person id (index into the people dictionary).
    pub id: i64,
    /// Display name.
    pub name: String,
    /// The developer's lines alive at the last sample.
    pub current_lines: i64,
    /// Lines the developer wrote that lived until a sample boundary.
    pub peak_lines: i64,
    /// `current_lines / peak_lines`.
    pub survival_rate: f64,
}

impl DeveloperSurvival {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("id", GoValue::Int(self.id));
        m.push("name", GoValue::Str(self.name.clone()));
        m.push("current_lines", GoValue::Int(self.current_lines));
        m.push("peak_lines", GoValue::Int(self.peak_lines));
        m.push("survival_rate", GoValue::Float(self.survival_rate));
        GoValue::Map(m)
    }
}

/// One cell of the developer interaction matrix (`--burndown-people`): lines
/// written by `author` that `modifier` deleted or replaced within the window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Interaction {
    /// The author of the affected lines.
    pub author_id: i64,
    /// The author's display name.
    pub author_name: String,
    /// The developer who deleted or replaced them.
    pub modifier_id: i64,
    /// The modifier's display name.
    pub modifier_name: String,
    /// Number of lines.
    pub lines_modified: i64,
    /// `author_id == modifier_id`.
    pub is_self_modify: bool,
}

impl Interaction {
    fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("author_id", GoValue::Int(self.author_id));
        m.push("author_name", GoValue::Str(self.author_name.clone()));
        m.push("modifier_id", GoValue::Int(self.modifier_id));
        m.push("modifier_name", GoValue::Str(self.modifier_name.clone()));
        m.push("lines_modified", GoValue::Int(self.lines_modified));
        m.push("is_self_modify", GoValue::Bool(self.is_self_modify));
        GoValue::Map(m)
    }
}

/// All computed burndown metric results.
///
/// `file_survival`, `developer_survival` and `interactions` are empty when
/// the corresponding tracking flag is off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputedMetrics {
    /// `aggregate`.
    pub aggregate: AggregateData,
    /// `global_survival`, one entry per sample.
    pub global_survival: Vec<SurvivalData>,
    /// `file_survival` (`--burndown-files`).
    pub file_survival: Vec<FileSurvival>,
    /// `developer_survival` (`--burndown-people`).
    pub developer_survival: Vec<DeveloperSurvival>,
    /// `interactions` (`--burndown-people`).
    pub interactions: Vec<Interaction>,
}

impl ComputedMetrics {
    /// Build the value tree every machine format (json, bin, yaml) encodes.
    ///
    /// ```
    /// use cf_analyzer_burndown::ComputedMetrics;
    ///
    /// let m = ComputedMetrics::default();
    /// let json = String::from_utf8(cf_gojson::marshal(&m.to_go_value())).unwrap();
    /// assert!(json.contains(r#""interactions":[]"#));
    /// let yaml = String::from_utf8(cf_goyaml::marshal(&m.to_go_value())).unwrap();
    /// assert!(yaml.contains("interactions: []"));
    /// ```
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut m = GoMap::new_struct();
        m.push("aggregate", self.aggregate.to_go_value());
        m.push(
            "global_survival",
            GoValue::Array(
                self.global_survival
                    .iter()
                    .map(SurvivalData::to_go_value)
                    .collect(),
            ),
        );
        m.push(
            "file_survival",
            GoValue::Array(
                self.file_survival
                    .iter()
                    .map(FileSurvival::to_go_value)
                    .collect(),
            ),
        );
        m.push(
            "developer_survival",
            GoValue::Array(
                self.developer_survival
                    .iter()
                    .map(DeveloperSurvival::to_go_value)
                    .collect(),
            ),
        );
        m.push(
            "interactions",
            GoValue::Array(
                self.interactions
                    .iter()
                    .map(Interaction::to_go_value)
                    .collect(),
            ),
        );
        GoValue::Map(m)
    }
}

/// An integer list value.
fn int_array(values: &[i64]) -> GoValue {
    GoValue::Array(values.iter().map(|&v| GoValue::Int(v)).collect())
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
            file_survival: Vec::new(),
            developer_survival: Vec::new(),
            interactions: Vec::new(),
        }
    }

    #[test]
    fn json_matches_the_documented_shape() {
        let bytes = cf_gojson::marshal(&head_metrics(112_539).to_go_value());
        let s = String::from_utf8(bytes).unwrap();
        assert_eq!(
            s,
            r#"{"aggregate":{"total_current_lines":112539,"total_peak_lines":112539,"overall_survival_rate":1,"analysis_period_days":0,"num_bands":1,"num_samples":1,"tracked_files":0,"tracked_developers":0},"global_survival":[{"sample_index":0,"total_lines":112539,"survival_rate":1,"band_breakdown":[112539]}],"file_survival":[],"developer_survival":[],"interactions":[]}"#
        );
    }

    #[test]
    fn yaml_and_json_agree_on_empty_lists() {
        let bytes = cf_goyaml::marshal(&head_metrics(112_539).to_go_value());
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("interactions: []"), "{s}");
        assert!(s.contains("file_survival: []"), "{s}");
        assert!(s.contains("developer_survival: []"), "{s}");
        assert!(s.contains("    total_current_lines: 112539"), "{s}");
        assert!(s.contains("        - 112539"), "{s}");
    }

    #[test]
    fn file_survival_emits_ownership_only_when_present() {
        let plain = FileSurvival {
            path: "a.go".into(),
            current_lines: 3,
            peak_lines: 4,
            survival_rate: 0.75,
            ownership: None,
        };
        let s = String::from_utf8(cf_gojson::marshal(&plain.to_go_value())).unwrap();
        assert_eq!(
            s,
            r#"{"path":"a.go","current_lines":3,"peak_lines":4,"survival_rate":0.75}"#
        );

        let owned = FileSurvival {
            ownership: Some(FileOwnership {
                lines_by_person: BTreeMap::from([(0, 2), (10, 1)]),
                top_owner: Some((0, "alice".into(), 66.5)),
            }),
            ..plain.clone()
        };
        let s = String::from_utf8(cf_gojson::marshal(&owned.to_go_value())).unwrap();
        assert_eq!(
            s,
            r#"{"path":"a.go","current_lines":3,"peak_lines":4,"survival_rate":0.75,"ownership":{"0":2,"10":1},"top_owner_id":0,"top_owner_name":"alice","top_owner_percentage":66.5}"#
        );

        let dead = FileSurvival {
            ownership: Some(FileOwnership::default()),
            ..plain
        };
        let s = String::from_utf8(cf_gojson::marshal(&dead.to_go_value())).unwrap();
        assert!(s.ends_with(r#""ownership":{}}"#), "{s}");
    }

    #[test]
    fn developer_and_interaction_rows_keep_field_order() {
        let d = DeveloperSurvival {
            id: 1,
            name: "bob".into(),
            current_lines: 4,
            peak_lines: 9,
            survival_rate: 0.5,
        };
        let s = String::from_utf8(cf_gojson::marshal(&d.to_go_value())).unwrap();
        assert_eq!(
            s,
            r#"{"id":1,"name":"bob","current_lines":4,"peak_lines":9,"survival_rate":0.5}"#
        );
        let i = Interaction {
            author_id: 0,
            author_name: "alice".into(),
            modifier_id: 0,
            modifier_name: "alice".into(),
            lines_modified: 7,
            is_self_modify: true,
        };
        let s = String::from_utf8(cf_gojson::marshal(&i.to_go_value())).unwrap();
        assert_eq!(
            s,
            r#"{"author_id":0,"author_name":"alice","modifier_id":0,"modifier_name":"alice","lines_modified":7,"is_self_modify":true}"#
        );
    }
}

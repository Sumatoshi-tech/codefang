//! Merge configuration values into the analyzer facts map.
//!
//! The fact-key strings and the apply rules (positive-only for numerics,
//! non-empty for strings, always for bools) are frozen: analyzer wiring
//! depends on them (compatibility contract).

use crate::types::{
    AnomalyConfig, BurndownConfig, ClonesConfig, Config, CouplesConfig, DevsConfig,
    FileHistoryConfig, ImportsConfig, SentimentConfig, ShotnessConfig,
};
use std::collections::HashMap;

/// A fact value stored in the analyzer facts map.
///
/// Numeric facts are stored as `Int` (for integer config fields) or `Float`
/// (for floating-point config fields), strings as `Str`, and booleans as
/// `Bool`.
#[derive(Debug, Clone, PartialEq)]
pub enum FactValue {
    /// Integer fact.
    Int(i64),
    /// Floating-point fact.
    Float(f64),
    /// String fact.
    Str(String),
    /// Boolean fact.
    Bool(bool),
}

impl FactValue {
    /// Returns the integer value if this fact is an [`FactValue::Int`].
    #[must_use]
    pub const fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns the float value if this fact is a [`FactValue::Float`].
    #[must_use]
    pub const fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(v) => Some(*v),
            _ => None,
        }
    }

    /// Returns the string value if this fact is a [`FactValue::Str`].
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the boolean value if this fact is a [`FactValue::Bool`].
    #[must_use]
    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

/// The analyzer facts map.
pub type Facts = HashMap<String, FactValue>;

/// Sets `facts[key] = value` when an integer value is positive (`> 0`).
///
/// Zero values are skipped so the analyzer keeps its built-in default.
fn apply_positive_int(facts: &mut Facts, key: &str, value: i64) {
    if value > 0 {
        facts.insert(key.to_owned(), FactValue::Int(value));
    }
}

/// Sets `facts[key] = value` when a float value is positive (`> 0`).
fn apply_positive_float(facts: &mut Facts, key: &str, value: f64) {
    if value > 0.0 {
        facts.insert(key.to_owned(), FactValue::Float(value));
    }
}

/// Sets `facts[key] = value` when a string is non-empty.
fn apply_non_empty(facts: &mut Facts, key: &str, value: &str) {
    if !value.is_empty() {
        facts.insert(key.to_owned(), FactValue::Str(value.to_owned()));
    }
}

/// Sets `facts[key] = value` unconditionally.
///
/// Boolean fields are always applied because `false` is a meaningful override.
fn apply_bool(facts: &mut Facts, key: &str, value: bool) {
    facts.insert(key.to_owned(), FactValue::Bool(value));
}

impl Config {
    /// Merges config values into the analyzer `facts` map.
    ///
    /// Only non-zero numeric/string config values override existing facts
    /// (zero means "use analyzer default" and is skipped); boolean fields are
    /// always applied because `false` is a meaningful value.
    pub fn apply_to_facts(&self, facts: &mut Facts) {
        self.apply_burndown_facts(facts);
        self.apply_couples_facts(facts);
        self.apply_devs_facts(facts);
        self.apply_file_history_facts(facts);
        self.apply_imports_facts(facts);
        self.apply_sentiment_facts(facts);
        self.apply_shotness_facts(facts);
        self.apply_typos_facts(facts);
        self.apply_anomaly_facts(facts);
        self.apply_clones_facts(facts);
    }

    fn apply_burndown_facts(&self, facts: &mut Facts) {
        let bd: &BurndownConfig = &self.history.burndown;
        apply_positive_int(facts, "Burndown.Granularity", bd.granularity);
        apply_positive_int(facts, "Burndown.Sampling", bd.sampling);
        apply_bool(facts, "Burndown.TrackFiles", bd.track_files);
        apply_bool(facts, "Burndown.TrackPeople", bd.track_people);
        apply_positive_int(facts, "Burndown.HibernationThreshold", bd.hibernation_threshold);
        apply_bool(facts, "Burndown.HibernationOnDisk", bd.hibernation_to_disk);
        apply_non_empty(facts, "Burndown.HibernationDirectory", &bd.hibernation_directory);
        apply_bool(facts, "Burndown.Debug", bd.debug);
        apply_positive_int(facts, "Burndown.Goroutines", bd.goroutines);
    }

    fn apply_couples_facts(&self, facts: &mut Facts) {
        let cp: &CouplesConfig = &self.history.couples;
        apply_positive_int(facts, "Couples.CouplingThresholdHigh", cp.coupling_threshold_high);
        apply_positive_int(facts, "Couples.OwnershipFewThreshold", cp.ownership_few_threshold);
        apply_positive_int(
            facts,
            "Couples.OwnershipModerateThreshold",
            cp.ownership_moderate_threshold,
        );
        apply_positive_int(facts, "Couples.BatchCouplingThreshold", cp.batch_coupling_threshold);
        apply_positive_int(facts, "Couples.HLLPrecision", cp.hll_precision);
        apply_positive_int(facts, "Couples.TopKPerFile", cp.top_k_per_file);
        apply_positive_int(facts, "Couples.MinEdgeWeight", cp.min_edge_weight);
    }

    fn apply_devs_facts(&self, facts: &mut Facts) {
        let dv: &DevsConfig = &self.history.devs;
        apply_bool(facts, "Devs.ConsiderEmptyCommits", dv.consider_empty_commits);
        apply_bool(facts, "Devs.Anonymize", dv.anonymize);
        apply_positive_float(facts, "Devs.BusFactorThreshold", dv.bus_factor_threshold);
        apply_positive_float(facts, "Devs.RiskThresholdCritical", dv.risk_threshold_critical);
        apply_positive_float(facts, "Devs.RiskThresholdHigh", dv.risk_threshold_high);
        apply_positive_float(facts, "Devs.RiskThresholdMedium", dv.risk_threshold_medium);
        apply_positive_float(facts, "Devs.ActiveThresholdRatio", dv.active_threshold_ratio);
        apply_positive_int(facts, "Devs.DefaultActiveDays", dv.default_active_days);
        apply_positive_int(facts, "Devs.HLLPrecision", dv.hll_precision);
    }

    fn apply_file_history_facts(&self, facts: &mut Facts) {
        let fh: &FileHistoryConfig = &self.history.file_history;
        apply_positive_int(
            facts,
            "FileHistory.HotspotThresholdCritical",
            fh.hotspot_threshold_critical,
        );
        apply_positive_int(facts, "FileHistory.HotspotThresholdHigh", fh.hotspot_threshold_high);
        apply_positive_int(
            facts,
            "FileHistory.HotspotThresholdMedium",
            fh.hotspot_threshold_medium,
        );
    }

    fn apply_imports_facts(&self, facts: &mut Facts) {
        let im: &ImportsConfig = &self.history.imports;
        apply_positive_int(facts, "Imports.Goroutines", im.goroutines);
        apply_positive_int(facts, "Imports.MaxFileSize", im.max_file_size);
        apply_positive_int(facts, "Imports.MaxDependencyRiskRows", im.max_dependency_risk_rows);
    }

    fn apply_sentiment_facts(&self, facts: &mut Facts) {
        let se: &SentimentConfig = &self.history.sentiment;
        apply_positive_int(facts, "CommentSentiment.MinLength", se.min_comment_length);
        apply_positive_float(facts, "CommentSentiment.Gap", se.gap);
        apply_positive_float(facts, "CommentSentiment.NeutralizerWeight", se.neutralizer_weight);
        apply_positive_float(facts, "CommentSentiment.MaxWeightRatio", se.max_weight_ratio);
        apply_positive_float(facts, "CommentSentiment.PositiveThreshold", se.positive_threshold);
        apply_positive_float(facts, "CommentSentiment.NegativeThreshold", se.negative_threshold);
        apply_positive_float(facts, "CommentSentiment.TrendThreshold", se.trend_threshold);
        apply_positive_float(
            facts,
            "CommentSentiment.LowSentimentRiskThreshold",
            se.low_sentiment_risk_thresh,
        );
    }

    fn apply_shotness_facts(&self, facts: &mut Facts) {
        let sh: &ShotnessConfig = &self.history.shotness;
        apply_non_empty(facts, "Shotness.DSLStruct", &sh.dsl_struct);
        apply_non_empty(facts, "Shotness.DSLName", &sh.dsl_name);
    }

    fn apply_typos_facts(&self, facts: &mut Facts) {
        apply_positive_int(
            facts,
            "TyposDatasetBuilder.MaximumAllowedDistance",
            self.history.typos.max_distance,
        );
    }

    fn apply_anomaly_facts(&self, facts: &mut Facts) {
        let an: &AnomalyConfig = &self.history.anomaly;
        apply_positive_float(facts, "TemporalAnomaly.Threshold", an.threshold);
        apply_positive_int(facts, "TemporalAnomaly.WindowSize", an.window_size);
    }

    fn apply_clones_facts(&self, facts: &mut Facts) {
        let cl: &ClonesConfig = &self.history.clones;
        apply_positive_int(facts, "Clones.MaxClonePairs", cl.max_clone_pairs);
        apply_positive_int(facts, "Clones.NumHashes", cl.num_hashes);
        apply_positive_int(facts, "Clones.NumBands", cl.num_bands);
        apply_positive_int(facts, "Clones.NumRows", cl.num_rows);
        apply_positive_int(facts, "Clones.ShingleSize", cl.shingle_size);
        apply_positive_float(facts, "Clones.SimilarityType2", cl.similarity_type2);
        apply_positive_float(facts, "Clones.SimilarityType3", cl.similarity_type3);
        apply_positive_float(facts, "Clones.ThresholdRatioYellow", cl.threshold_ratio_yellow);
        apply_positive_float(facts, "Clones.ThresholdRatioRed", cl.threshold_ratio_red);
        apply_positive_int(facts, "Clones.ThresholdPairsYellow", cl.threshold_pairs_yellow);
        apply_positive_int(facts, "Clones.ThresholdPairsRed", cl.threshold_pairs_red);
    }
}

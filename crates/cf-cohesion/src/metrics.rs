//! Computed metrics: the view serialized by the `yaml` / `bin` machine
//! formats, derived from an [`Aggregate`].
//!
//! Field order of every struct is the serialized key order.

use std::collections::BTreeMap;

use crate::aggregator::{Aggregate, AggregatedFunction};

// --- Quality thresholds on per-function cohesion ---

/// Cohesion at or above this is "Excellent".
pub const COHESION_THRESHOLD_EXCELLENT: f64 = 0.6;
/// Cohesion at or above this is "Good".
pub const COHESION_THRESHOLD_GOOD: f64 = 0.4;
/// Cohesion at or above this is "Fair".
pub const COHESION_THRESHOLD_FAIR: f64 = 0.3;
/// Health score = cohesion score x this multiplier.
pub const HEALTH_SCORE_MULTIPLIER: f64 = 100.0;

/// The LCOM variant name reported in `aggregate.lcom_variant`.
pub const LCOM_VARIANT: &str = "LCOM-HS (Henderson-Sellers)";

// --- Distribution keys ---

/// Distribution bucket key.
pub const METRIC_DIST_EXCELLENT: &str = "excellent";
/// Distribution bucket key.
pub const METRIC_DIST_GOOD: &str = "good";
/// Distribution bucket key.
pub const METRIC_DIST_FAIR: &str = "fair";
/// Distribution bucket key.
pub const METRIC_DIST_POOR: &str = "poor";

/// Per-function cohesion output: `name`, `source_file`, `cohesion`,
/// `quality_level`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FunctionCohesionData {
    /// `name`.
    pub name: String,
    /// `source_file`.
    pub source_file: String,
    /// `cohesion`.
    pub cohesion: f64,
    /// `quality_level`.
    pub quality_level: String,
}

/// Low-cohesion function output: `name`, `source_file`, `cohesion`,
/// `risk_level`, `recommendation`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LowCohesionFunctionData {
    /// `name`.
    pub name: String,
    /// `source_file`.
    pub source_file: String,
    /// `cohesion`.
    pub cohesion: f64,
    /// `risk_level` (`HIGH` / `MEDIUM`).
    pub risk_level: String,
    /// `recommendation`.
    pub recommendation: String,
}

/// Aggregate summary: `total_functions`, `lcom`, `lcom_variant`,
/// `cohesion_score`, `function_cohesion`, `health_score`, `message`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AggregateData {
    /// `total_functions`.
    pub total_functions: i64,
    /// `lcom`.
    pub lcom: f64,
    /// `lcom_variant`.
    pub lcom_variant: String,
    /// `cohesion_score`.
    pub cohesion_score: f64,
    /// `function_cohesion`.
    pub function_cohesion: f64,
    /// `health_score`.
    pub health_score: f64,
    /// `message`.
    pub message: String,
}

/// All computed metrics: `function_cohesion`, `distribution`,
/// `low_cohesion_functions`, `aggregate`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// Every function with its quality level, cohesion ascending.
    pub function_cohesion: Vec<FunctionCohesionData>,
    /// Bucket counts; only buckets that occur are present.
    pub distribution: BTreeMap<String, i64>,
    /// Functions below [`COHESION_THRESHOLD_GOOD`], cohesion ascending.
    pub low_cohesion_functions: Vec<LowCohesionFunctionData>,
    /// Summary statistics.
    pub aggregate: AggregateData,
}

/// Quality label of a cohesion value: the first label whose threshold the
/// value meets, scanning highest first; "Poor" otherwise.
#[must_use]
pub fn classify_cohesion_quality(cohesion: f64) -> &'static str {
    if cohesion >= COHESION_THRESHOLD_EXCELLENT {
        "Excellent"
    } else if cohesion >= COHESION_THRESHOLD_GOOD {
        "Good"
    } else if cohesion >= COHESION_THRESHOLD_FAIR {
        "Fair"
    } else {
        "Poor"
    }
}

/// Distribution bucket key of a cohesion value.
#[must_use]
pub fn classify_cohesion_level(cohesion: f64) -> &'static str {
    if cohesion >= COHESION_THRESHOLD_EXCELLENT {
        METRIC_DIST_EXCELLENT
    } else if cohesion >= COHESION_THRESHOLD_GOOD {
        METRIC_DIST_GOOD
    } else if cohesion >= COHESION_THRESHOLD_FAIR {
        METRIC_DIST_FAIR
    } else {
        METRIC_DIST_POOR
    }
}

/// Orders functions by cohesion ascending, then name, then source file.
fn by_cohesion_then_name(a: &AggregatedFunction, b: &AggregatedFunction) -> std::cmp::Ordering {
    a.cohesion
        .total_cmp(&b.cohesion)
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.source_file.cmp(&b.source_file))
}

/// Computes the machine-format metrics of an aggregate.
#[must_use]
pub fn compute_metrics(agg: &Aggregate) -> ComputedMetrics {
    let mut ordered: Vec<&AggregatedFunction> = agg.functions.iter().collect();
    ordered.sort_by(|a, b| by_cohesion_then_name(a, b));

    let function_cohesion = ordered
        .iter()
        .map(|f| FunctionCohesionData {
            name: f.name.clone(),
            source_file: f.source_file.clone(),
            cohesion: f.cohesion,
            quality_level: classify_cohesion_quality(f.cohesion).to_string(),
        })
        .collect();

    let mut distribution: BTreeMap<String, i64> = BTreeMap::new();
    for f in &ordered {
        *distribution
            .entry(classify_cohesion_level(f.cohesion).to_string())
            .or_insert(0) += 1;
    }

    let low_cohesion_functions = ordered
        .iter()
        .filter(|f| f.cohesion < COHESION_THRESHOLD_GOOD)
        .map(|f| {
            let (risk_level, recommendation) = if f.cohesion < COHESION_THRESHOLD_FAIR {
                ("HIGH", "Consider splitting into multiple focused functions")
            } else {
                (
                    "MEDIUM",
                    "Review function responsibilities for possible separation",
                )
            };
            LowCohesionFunctionData {
                name: f.name.clone(),
                source_file: f.source_file.clone(),
                cohesion: f.cohesion,
                risk_level: risk_level.to_string(),
                recommendation: recommendation.to_string(),
            }
        })
        .collect();

    ComputedMetrics {
        function_cohesion,
        distribution,
        low_cohesion_functions,
        aggregate: AggregateData {
            total_functions: agg.total_functions as i64,
            lcom: agg.lcom,
            lcom_variant: LCOM_VARIANT.to_string(),
            cohesion_score: agg.cohesion_score,
            function_cohesion: agg.function_cohesion,
            health_score: agg.cohesion_score * HEALTH_SCORE_MULTIPLIER,
            message: agg.message.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn func(name: &str, file: &str, cohesion: f64) -> AggregatedFunction {
        AggregatedFunction {
            source_file: file.into(),
            name: name.into(),
            start_line: 1,
            line_count: 1,
            variable_count: 1,
            cohesion,
        }
    }

    fn agg(functions: Vec<AggregatedFunction>) -> Aggregate {
        Aggregate {
            files_analyzed: 1,
            files_with_functions: 1,
            total_functions: functions.len(),
            lcom: 0.2,
            cohesion_score: 0.8,
            function_cohesion: 0.5,
            functions,
            message: "msg".into(),
        }
    }

    #[test]
    fn quality_classification() {
        assert_eq!(classify_cohesion_quality(0.6), "Excellent");
        assert_eq!(classify_cohesion_quality(0.4), "Good");
        assert_eq!(classify_cohesion_quality(0.3), "Fair");
        assert_eq!(classify_cohesion_quality(0.0), "Poor");
    }

    #[test]
    fn distribution_only_includes_present_buckets() {
        let m = compute_metrics(&agg(vec![
            func("a", "a.go", 0.9),
            func("b", "a.go", 0.5),
            func("c", "a.go", 0.5),
        ]));
        assert_eq!(m.distribution.get("excellent"), Some(&1));
        assert_eq!(m.distribution.get("good"), Some(&2));
        assert_eq!(m.distribution.get("fair"), None);
        assert_eq!(m.distribution.get("poor"), None);
    }

    #[test]
    fn functions_sorted_ascending_and_low_cohesion_classified() {
        let m = compute_metrics(&agg(vec![
            func("good", "a.go", 0.9),
            func("mid", "a.go", 0.35),
            func("bad", "a.go", 0.1),
        ]));
        let names: Vec<&str> = m
            .function_cohesion
            .iter()
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(names, ["bad", "mid", "good"]);
        assert_eq!(m.low_cohesion_functions.len(), 2);
        assert_eq!(m.low_cohesion_functions[0].name, "bad");
        assert_eq!(m.low_cohesion_functions[0].risk_level, "HIGH");
        assert_eq!(m.low_cohesion_functions[1].name, "mid");
        assert_eq!(m.low_cohesion_functions[1].risk_level, "MEDIUM");
    }

    #[test]
    fn aggregate_carries_health_score_and_variant() {
        let m = compute_metrics(&agg(vec![]));
        assert!((m.aggregate.health_score - 80.0).abs() < 1e-9);
        assert_eq!(m.aggregate.lcom_variant, LCOM_VARIANT);
        assert_eq!(m.aggregate.message, "msg");
    }
}

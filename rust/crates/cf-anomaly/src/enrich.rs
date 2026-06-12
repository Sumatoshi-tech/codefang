//! Cross-analyzer enrichment orchestration (the pure detection logic).
//!
//! The store-IO wrapper belongs to the `cf-analyze` store layer and is
//! tracked as a framework dependency (see crate todos).

use std::collections::BTreeMap;

use crate::detect::{detect_external_anomalies, sort_external_results};
use crate::model::{ExternalAnomaly, ExternalSummary};

/// Analyzer ID that is excluded from enrichment (the anomaly analyzer
/// itself).
pub const ANALYZER_NAME_ANOMALY: &str = "anomaly";

/// One analyzer's extracted time series: the tick axis plus a map of named
/// dimensions to per-tick values.
#[derive(Debug, Clone, Default)]
pub struct ExtractedSeries {
    /// Tick axis shared by every dimension.
    pub ticks: Vec<i64>,
    /// Dimension name -> per-tick values.
    pub dimensions: BTreeMap<String, Vec<f64>>,
}

/// Runs cross-analyzer enrichment over already-extracted time series.
///
/// For each `(analyzer_id, series)` (skipping the anomaly analyzer itself and
/// any empty series), it detects external anomalies and summaries,
/// accumulates them, then sorts: anomalies by descending absolute Z-score and
/// summaries by `(source, dimension)`.
///
/// The caller supplies the already-extracted series so this function stays
/// pure and independent of the store layer.
#[must_use]
pub fn run_store_enrichment(
    extracted: &BTreeMap<String, ExtractedSeries>,
    window_size: usize,
    threshold: f64,
) -> (Vec<ExternalAnomaly>, Vec<ExternalSummary>) {
    let mut all_anomalies = Vec::new();
    let mut all_summaries = Vec::new();

    for (analyzer_id, series) in extracted {
        // Skip the anomaly analyzer itself.
        if analyzer_id == ANALYZER_NAME_ANOMALY {
            continue;
        }

        if series.ticks.is_empty() || series.dimensions.is_empty() {
            continue;
        }

        let (anomalies, summaries) = detect_external_anomalies(
            analyzer_id,
            &series.ticks,
            &series.dimensions,
            window_size,
            threshold,
        );
        all_anomalies.extend(anomalies);
        all_summaries.extend(summaries);
    }

    sort_external_results(&mut all_anomalies, &mut all_summaries);

    (all_anomalies, all_summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(ticks: Vec<i64>, dims: &[(&str, Vec<f64>)]) -> ExtractedSeries {
        ExtractedSeries {
            ticks,
            dimensions: dims
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        }
    }

    #[test]
    fn basic_detects_spike() {
        // Mirrors reference test TestEnrichFromStore_Basic.
        let extracted = BTreeMap::from([(
            "test-source".to_string(),
            series(vec![0, 1, 2, 3, 4], &[("metric_a", vec![1.0, 1.0, 1.0, 1.0, 100.0])]),
        )]);

        let (anomalies, _) = run_store_enrichment(&extracted, 3, 2.0);
        assert!(!anomalies.is_empty());

        let found = anomalies.iter().any(|a| {
            a.source == "test-source"
                && a.dimension == "metric_a"
                && a.tick == 4
                && a.z_score > 2.0
                && (a.raw_value - 100.0).abs() < 0.001
        });
        assert!(found, "expected anomaly at tick 4");
    }

    #[test]
    fn empty_store_yields_nothing() {
        // Mirrors reference test TestEnrichFromStore_EmptyStore (no matching analyzers).
        let extracted: BTreeMap<String, ExtractedSeries> = BTreeMap::new();
        let (anomalies, summaries) = run_store_enrichment(&extracted, 3, 2.0);
        assert!(anomalies.is_empty());
        assert!(summaries.is_empty());
    }

    #[test]
    fn skips_anomaly_analyzer() {
        // Mirrors reference test TestEnrichFromStore_SkipsAnomalyAnalyzer.
        let extracted = BTreeMap::from([(
            "anomaly".to_string(),
            series(vec![0, 1], &[("dim", vec![1.0, 100.0])]),
        )]);

        let (anomalies, _) = run_store_enrichment(&extracted, 3, 2.0);
        assert!(anomalies.is_empty(), "anomaly analyzer should be skipped");
    }

    #[test]
    fn equivalence_to_direct_detection() {
        // Mirrors reference test TestEnrichFromStore_Equivalence.
        let ticks = vec![0, 1, 2, 3, 4, 5];
        let dims = BTreeMap::from([
            ("dim_a".to_string(), vec![1.0, 1.0, 1.0, 1.0, 50.0, 1.0]),
            ("dim_b".to_string(), vec![5.0, 5.0, 5.0, 5.0, 5.0, 5.0]),
        ]);

        let (expected_anomalies, expected_summaries) =
            detect_external_anomalies("equiv-src", &ticks, &dims, 3, 2.0);

        let extracted = BTreeMap::from([(
            "equiv-src".to_string(),
            ExtractedSeries { ticks: ticks.clone(), dimensions: dims.clone() },
        )]);
        let (store_anomalies, store_summaries) = run_store_enrichment(&extracted, 3, 2.0);

        // Single source => the global sort preserves the same set.
        assert_eq!(store_anomalies.len(), expected_anomalies.len());
        assert_eq!(store_summaries.len(), expected_summaries.len());
        for (got, want) in store_summaries.iter().zip(expected_summaries.iter()) {
            assert_eq!(got.source, want.source);
            assert_eq!(got.dimension, want.dimension);
            assert!((got.mean - want.mean).abs() < 0.001);
            assert!((got.stddev - want.stddev).abs() < 0.001);
            assert_eq!(got.anomalies, want.anomalies);
        }
    }
}

//! Cross-file result aggregation (`aggregator.go`).
//!
//! The Go `Aggregator` embeds `common.Aggregator` + a `DetailedDataCollector`
//! and overrides `Aggregate`/`GetResult` to additionally collect every detailed
//! function across files (deduplicating by the composite key
//! `["_source_file", "name"]` so same-named functions in different files are
//! preserved). The base aggregator machinery lives in `cf-analyzers-common`.
//!
//! This module ports the *self-contained* pieces verbatim — the numeric/count
//! key lists, the volume-only message labeler, and the empty-result builder —
//! and exposes the configuration the base aggregator is constructed from. The
//! base `Aggregator`/`DetailedDataCollector` wiring is supplied by
//! `cf-analyzers-common` at integration time (see crate todos for the exact
//! constructor surface to bind against).

use cf_gojson::{GoMap, GoValue};

// --- Volume thresholds (aggregator.go) ---
const MAGIC_100: f64 = 100.0;
const MAGIC_1000: f64 = 1000.0;
const VOLUME_THRESHOLD_HIGH: f64 = 5000.0;

/// Numeric metric keys averaged/summed by the base aggregator (`getNumericKeys`).
/// Order matches the Go slice.
#[must_use]
pub fn numeric_keys() -> &'static [&'static str] {
    &[
        "volume",
        "difficulty",
        "effort",
        "time_to_program",
        "delivered_bugs",
        "distinct_operators",
        "distinct_operands",
        "total_operators",
        "total_operands",
        "vocabulary",
        "length",
        "estimated_length",
    ]
}

/// Count keys summed by the base aggregator (`getCountKeys`).
#[must_use]
pub fn count_keys() -> &'static [&'static str] {
    &["total_functions"]
}

/// The composite key by which detailed functions are deduplicated across files
/// (`["_source_file", "name"]`).
#[must_use]
pub fn detailed_dedup_key() -> &'static [&'static str] {
    &["_source_file", "name"]
}

/// The collection key holding the detailed function list (`"functions"`).
pub const DETAILED_COLLECTION_KEY: &str = "functions";

/// Builds the volume-based aggregate message (`buildHalsteadMessage`).
///
/// Threshold-labeler semantics (highest matching limit first; a value `>= limit`
/// takes that label): `>=5000` very high, `>=1000` high, `>=100` moderate,
/// otherwise low.
#[must_use]
pub fn build_halstead_message(volume: f64) -> &'static str {
    if volume >= VOLUME_THRESHOLD_HIGH {
        "Very high Halstead complexity - significant refactoring recommended"
    } else if volume >= MAGIC_1000 {
        "High Halstead complexity - consider refactoring"
    } else if volume >= MAGIC_100 {
        "Moderate Halstead complexity - acceptable"
    } else {
        "Low Halstead complexity - well-structured code"
    }
}

/// Builds the empty aggregate result (`buildEmptyHalsteadResult`). Mirrors
/// [`crate::report::build_empty_result`] with the fixed message
/// `"No functions found"`.
#[must_use]
pub fn build_empty_halstead_result() -> GoValue {
    let mut m = GoMap::new_map();
    m.push("total_functions", GoValue::Int(0));
    m.push("volume", GoValue::Float(0.0));
    m.push("difficulty", GoValue::Float(0.0));
    m.push("effort", GoValue::Float(0.0));
    m.push("time_to_program", GoValue::Float(0.0));
    m.push("delivered_bugs", GoValue::Float(0.0));
    m.push("distinct_operators", GoValue::Int(0));
    m.push("distinct_operands", GoValue::Int(0));
    m.push("total_operators", GoValue::Int(0));
    m.push("total_operands", GoValue::Int(0));
    m.push("vocabulary", GoValue::Int(0));
    m.push("length", GoValue::Int(0));
    m.push("estimated_length", GoValue::Float(0.0));
    m.push("message", GoValue::Str("No functions found".to_string()));
    GoValue::Object(m)
}

/// Configuration values used to construct the base `common.Aggregator` for
/// Halstead. Held here so the integration layer can build the real aggregator
/// with byte-identical parameters.
#[derive(Debug, Clone)]
pub struct AggregatorConfig {
    /// Analyzer name (`"halstead"`).
    pub name: &'static str,
    /// Numeric keys.
    pub numeric_keys: &'static [&'static str],
    /// Count keys.
    pub count_keys: &'static [&'static str],
    /// Detailed collection key.
    pub collection_key: &'static str,
    /// Detailed dedup composite key.
    pub dedup_key: &'static [&'static str],
}

/// The Halstead aggregator configuration (`NewAggregator` parameters).
#[derive(Debug, Clone, Copy, Default)]
pub struct Aggregator;

impl Aggregator {
    /// Returns the configuration the base `common.Aggregator` is built from.
    #[must_use]
    pub fn config() -> AggregatorConfig {
        AggregatorConfig {
            name: crate::ANALYZER_NAME,
            numeric_keys: numeric_keys(),
            count_keys: count_keys(),
            collection_key: DETAILED_COLLECTION_KEY,
            dedup_key: detailed_dedup_key(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `TestBuildHalsteadMessage`.
    #[test]
    fn build_message_tiers() {
        assert_eq!(build_halstead_message(50.0), "Low Halstead complexity - well-structured code");
        assert_eq!(build_halstead_message(99.0), "Low Halstead complexity - well-structured code");
        assert_eq!(build_halstead_message(100.0), "Moderate Halstead complexity - acceptable");
        assert_eq!(build_halstead_message(500.0), "Moderate Halstead complexity - acceptable");
        assert_eq!(build_halstead_message(999.0), "Moderate Halstead complexity - acceptable");
        assert_eq!(build_halstead_message(1000.0), "High Halstead complexity - consider refactoring");
        assert_eq!(build_halstead_message(4999.0), "High Halstead complexity - consider refactoring");
        assert_eq!(
            build_halstead_message(5000.0),
            "Very high Halstead complexity - significant refactoring recommended"
        );
        assert_eq!(
            build_halstead_message(10000.0),
            "Very high Halstead complexity - significant refactoring recommended"
        );
    }

    /// Ported from `TestHalsteadGetNumericKeys` / `TestHalsteadGetCountKeys`.
    #[test]
    fn key_lists() {
        let expected_numeric = [
            "volume", "difficulty", "effort", "time_to_program", "delivered_bugs",
            "distinct_operators", "distinct_operands", "total_operators", "total_operands",
            "vocabulary", "length", "estimated_length",
        ];
        assert_eq!(numeric_keys(), &expected_numeric);
        assert_eq!(count_keys(), &["total_functions"]);
    }

    /// Ported from `TestBuildEmptyHalsteadResult`.
    #[test]
    fn empty_result_fields() {
        let GoValue::Object(m) = build_empty_halstead_result() else {
            panic!("expected object")
        };
        for field in [
            "total_functions", "volume", "difficulty", "effort", "time_to_program",
            "delivered_bugs", "distinct_operators", "distinct_operands", "total_operators",
            "total_operands", "vocabulary", "length", "estimated_length", "message",
        ] {
            assert!(m.get(field).is_some(), "missing field {field}");
        }
        assert_eq!(m.get("total_functions"), Some(&GoValue::Int(0)));
        assert_eq!(m.get("message"), Some(&GoValue::Str("No functions found".to_string())));
    }
}

//! Report-contract serialization for the quality machine reports.
//!
//! All machine output is routed through [`cf_gojson`] (the report-format JSON
//! encoder) — never serde defaults; the bytes are pinned against the reference
//! binary by `tests/compat`. The CFB1 "bin" envelope is produced via
//! [`cf_reportutil::encode_binary_envelope`].
//!
//! The YAML report path is owned by the `cf-analyze` cross-format conversion
//! hub, which builds the YAML value tree through `cf-goyaml`; the per-analyzer
//! code only ever produces the dynamic report map (it is then routed to JSON /
//! YAML / NDJSON / CFB1 by `cf-analyze`). So this module exposes JSON + CFB1
//! helpers over the same [`GoValue`] tree the conversion hub consumes; a
//! `to_yaml` convenience is intentionally omitted here to avoid binding the
//! `cf-goyaml` value type at this layer.
//!
//! # Origin classification (the load-bearing rule)
//!
//! * [`TickStats`], [`TimeSeriesEntry`], [`AggregateData`], [`ComputedMetrics`]
//!   are **struct-origin**: their fields are emitted in declaration order
//!   (honoring `omitempty` on `start_time` / `end_time`). They are built with a
//!   [`GoMap`] in [`MapOrigin::Struct`] mode (insertion order preserved).
//! * The per-commit summary map ([`commit_summary_value`]) is **map-origin**:
//!   its keys are byte-sorted on encode. It is built with a [`GoMap`] in
//!   [`MapOrigin::Map`] mode.

use cf_gojson::{marshal, marshal_indent, GoMap, GoValue, MapOrigin};

use crate::metrics::{AggregateData, CommitSummary, ComputedMetrics, TickStats, TimeSeriesEntry};

fn struct_map() -> GoMap {
    GoMap::new(MapOrigin::Struct)
}

/// Builds the struct-origin [`GoValue`] for a [`TickStats`].
///
/// Field order matches the [`TickStats`] declaration order (the wire order).
#[must_use]
pub fn tick_stats_value(ts: &TickStats) -> GoValue {
    let mut m = struct_map();
    m.push("complexity_mean", GoValue::Float(ts.complexity_mean));
    m.push("complexity_median", GoValue::Float(ts.complexity_median));
    m.push("complexity_p95", GoValue::Float(ts.complexity_p95));
    m.push("complexity_max", GoValue::Float(ts.complexity_max));

    m.push("halstead_vol_mean", GoValue::Float(ts.halstead_vol_mean));
    m.push(
        "halstead_vol_median",
        GoValue::Float(ts.halstead_vol_median),
    );
    m.push("halstead_vol_p95", GoValue::Float(ts.halstead_vol_p95));
    m.push("halstead_vol_sum", GoValue::Float(ts.halstead_vol_sum));

    m.push("delivered_bugs_sum", GoValue::Float(ts.delivered_bugs_sum));

    m.push("comment_score_mean", GoValue::Float(ts.comment_score_mean));
    m.push("comment_score_min", GoValue::Float(ts.comment_score_min));
    m.push("doc_coverage_mean", GoValue::Float(ts.doc_coverage_mean));

    m.push("cohesion_mean", GoValue::Float(ts.cohesion_mean));
    m.push("cohesion_min", GoValue::Float(ts.cohesion_min));

    m.push("files_analyzed", GoValue::Int(ts.files_analyzed));
    m.push("total_functions", GoValue::Int(ts.total_functions));
    m.push("max_complexity", GoValue::Int(ts.max_complexity));
    GoValue::Object(m)
}

/// Builds the struct-origin [`GoValue`] for a [`TimeSeriesEntry`].
///
/// `start_time` / `end_time` are omitted when empty (`omitempty`).
#[must_use]
pub fn time_series_entry_value(e: &TimeSeriesEntry) -> GoValue {
    let mut m = struct_map();
    m.push("tick", GoValue::Int(e.tick));
    if !e.start_time.is_empty() {
        m.push("start_time", GoValue::Str(e.start_time.clone()));
    }
    if !e.end_time.is_empty() {
        m.push("end_time", GoValue::Str(e.end_time.clone()));
    }
    m.push("stats", tick_stats_value(&e.stats));
    GoValue::Object(m)
}

/// Builds the struct-origin [`GoValue`] for an [`AggregateData`].
#[must_use]
pub fn aggregate_value(a: &AggregateData) -> GoValue {
    let mut m = struct_map();
    m.push("total_ticks", GoValue::Int(a.total_ticks));
    m.push("total_files_analyzed", GoValue::Int(a.total_files_analyzed));
    m.push(
        "complexity_median_mean",
        GoValue::Float(a.complexity_median_mean),
    );
    m.push("complexity_p95_mean", GoValue::Float(a.complexity_p95_mean));
    m.push(
        "halstead_vol_median_mean",
        GoValue::Float(a.halstead_vol_median_mean),
    );
    m.push(
        "total_delivered_bugs",
        GoValue::Float(a.total_delivered_bugs),
    );
    m.push(
        "comment_score_mean_mean",
        GoValue::Float(a.comment_score_mean_mean),
    );
    m.push("min_comment_score", GoValue::Float(a.min_comment_score));
    m.push("cohesion_mean_mean", GoValue::Float(a.cohesion_mean_mean));
    m.push("min_cohesion", GoValue::Float(a.min_cohesion));
    GoValue::Object(m)
}

/// Builds the struct-origin [`GoValue`] for a [`ComputedMetrics`].
#[must_use]
pub fn computed_metrics_value(c: &ComputedMetrics) -> GoValue {
    let mut m = struct_map();
    m.push(
        "time_series",
        GoValue::Array(c.time_series.iter().map(time_series_entry_value).collect()),
    );
    m.push("aggregate", aggregate_value(&c.aggregate));
    GoValue::Object(m)
}

/// Builds the **map-origin** [`GoValue`] for a per-commit [`CommitSummary`].
///
/// The keys are byte-sorted on encode; build order is irrelevant because
/// [`MapOrigin::Map`] reorders.
#[must_use]
pub fn commit_summary_value(s: &CommitSummary) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Map);
    m.push("complexity_median", GoValue::Float(s.complexity_median));
    m.push("cognitive_median", GoValue::Float(s.cognitive_median));
    m.push("max_complexity", GoValue::Int(s.max_complexity));
    m.push("functions", GoValue::Int(s.functions));
    m.push("halstead_vol_median", GoValue::Float(s.halstead_vol_median));
    m.push(
        "halstead_effort_median",
        GoValue::Float(s.halstead_effort_median),
    );
    m.push("delivered_bugs_sum", GoValue::Float(s.delivered_bugs_sum));
    m.push("comment_score_min", GoValue::Float(s.comment_score_min));
    m.push("doc_coverage_mean", GoValue::Float(s.doc_coverage_mean));
    m.push("cohesion_min", GoValue::Float(s.cohesion_min));
    m.push("files_analyzed", GoValue::Int(s.files_analyzed));
    GoValue::Object(m)
}

/// Encodes a [`ComputedMetrics`] as indented JSON.
///
/// indent `"  "`, HTML-escape ON, **no trailing newline**. This matches the
/// analyzer-level `format_report_json` path used by the sibling analyzers; the
/// run/render dispatch that adds a trailing newline lives in the `cf-analyze`
/// `Encoder` builder.
#[must_use]
pub fn to_json_pretty(c: &ComputedMetrics) -> Vec<u8> {
    marshal_indent(&computed_metrics_value(c))
}

/// Encodes a [`ComputedMetrics`] as compact JSON.
///
/// Compact, HTML-escape ON, no trailing newline. This is the CFB1 payload and
/// the per-NDJSON-line body.
#[must_use]
pub fn to_json_compact(c: &ComputedMetrics) -> Vec<u8> {
    marshal(&computed_metrics_value(c))
}

/// Encodes a [`ComputedMetrics`] as a single CFB1 "bin" record.
///
/// `b"CFB1"` + LE u32 payload length + compact-JSON payload.
/// [`cf_reportutil::encode_binary_envelope`] marshals the [`GoValue`] tree
/// internally via `cf_gojson::marshal`, so the payload is the same compact,
/// HTML-escaped JSON bytes as [`to_json_compact`].
///
/// # Errors
///
/// Returns [`cf_reportutil::EncodeError`] if the marshalled payload exceeds the
/// CFB1 length-field cap (`MAX_PAYLOAD_SIZE`).
pub fn to_bin(c: &ComputedMetrics) -> Result<Vec<u8>, cf_reportutil::EncodeError> {
    cf_reportutil::encode_binary_envelope(&computed_metrics_value(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{ComputedMetrics, TickStats, TimeSeriesEntry};

    #[test]
    fn empty_metrics_compact_shape() {
        let s = String::from_utf8(to_json_compact(&ComputedMetrics::default())).unwrap();
        // time_series [] then aggregate {...}; compact (no space after colon).
        assert!(s.starts_with("{\"time_series\":[],\"aggregate\":{"));
        assert!(!s.contains(": "));
        assert!(!s.ends_with('\n'));
    }

    #[test]
    fn pretty_has_no_trailing_newline() {
        let s = String::from_utf8(to_json_pretty(&ComputedMetrics::default())).unwrap();
        assert!(s.starts_with("{\n  \"time_series\": [],"));
        assert!(!s.ends_with('\n'));
    }

    #[test]
    fn bin_envelope_header_and_payload() {
        let bytes = to_bin(&ComputedMetrics::default()).expect("envelope encodes");
        assert_eq!(&bytes[0..4], b"CFB1");
        let len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        assert_eq!(len, bytes.len() - 8);
        let payload = String::from_utf8(bytes[8..].to_vec()).unwrap();
        assert!(!payload.contains('\n'));
        assert!(payload.contains("\"time_series\":[]"));
    }

    #[test]
    fn tick_stats_field_order_preserved() {
        let ts = TickStats {
            complexity_mean: 1.0,
            files_analyzed: 5,
            max_complexity: 9,
            ..TickStats::default()
        };
        let s = String::from_utf8(marshal(&tick_stats_value(&ts))).unwrap();
        // complexity_mean first, max_complexity last (declaration order).
        let pos_first = s.find("complexity_mean").unwrap();
        let pos_last = s.find("max_complexity").unwrap();
        assert!(pos_first < pos_last);
        assert!(s.starts_with("{\"complexity_mean\":1,"));
    }

    #[test]
    fn commit_summary_keys_byte_sorted() {
        let summary = crate::metrics::CommitSummary {
            complexity_median: 1.0,
            cognitive_median: 2.0,
            max_complexity: 3,
            functions: 4,
            halstead_vol_median: 5.0,
            halstead_effort_median: 6.0,
            delivered_bugs_sum: 7.0,
            comment_score_min: 8.0,
            doc_coverage_mean: 9.0,
            cohesion_min: 10.0,
            files_analyzed: 11,
        };
        let s = String::from_utf8(marshal(&commit_summary_value(&summary))).unwrap();
        // map-origin: cognitive_median < cohesion_min < comment_score_min < ...
        let p_cognitive = s.find("cognitive_median").unwrap();
        let p_cohesion = s.find("cohesion_min").unwrap();
        let p_comment = s.find("comment_score_min").unwrap();
        let p_complexity = s.find("complexity_median").unwrap();
        assert!(p_cognitive < p_cohesion);
        assert!(p_cohesion < p_comment);
        assert!(p_comment < p_complexity);
    }

    #[test]
    fn time_series_entry_omitempty_times() {
        let mut e = TimeSeriesEntry {
            tick: 0,
            ..TimeSeriesEntry::default()
        };
        let s = String::from_utf8(marshal(&time_series_entry_value(&e))).unwrap();
        assert!(!s.contains("start_time"));
        assert!(!s.contains("end_time"));

        e.start_time = "2024-01-01T00:00:00Z".into();
        let s = String::from_utf8(marshal(&time_series_entry_value(&e))).unwrap();
        assert!(s.contains("\"start_time\":\"2024-01-01T00:00:00Z\""));
        assert!(!s.contains("end_time"));
    }
}

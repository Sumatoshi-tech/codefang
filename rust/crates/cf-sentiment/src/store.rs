//! Structured store record kinds and the store-write payload mapping.
//!
//! Ports the kind constants from `store_writer.go` and the pure portion of
//! `WriteToStore` (the record set derived from [`ComputedMetrics`]). It also
//! ports the store-based time-series extraction (`store_reader.go`'s
//! `extractStoreTimeSeries`) used by the cross-analyzer anomaly path.
//!
//! The actual `ReportWriter`/`ReportReader` I/O lives in the not-yet-ported
//! `cf-analyze` store layer; mirroring the `cf-anomaly` port, this module exposes
//! the pure record view + extraction logic that a store adapter drives.

use crate::metrics::DIM_SENTIMENT;
use crate::model::{AggregateData, ComputedMetrics, TimeSeriesData, TrendData};

/// Record kind: per-tick `TimeSeriesData` records (sorted by tick).
pub const KIND_TIME_SERIES: &str = "time_series";
/// Record kind: the single `TrendData` record.
pub const KIND_TREND: &str = "trend";
/// Record kind: the single `AggregateData` record.
pub const KIND_AGGREGATE: &str = "aggregate";

/// The set of store records derived from computed metrics.
///
/// Mirrors the records `WriteToStore` streams: the `time_series` slice, the
/// single `trend` record, and the single `aggregate` record. Borrowing keeps
/// this allocation-free; a store-writer adapter (in `cf-analyze`, once ported)
/// iterates these and writes each kind through the byte-identical codec.
pub struct StoreRecords<'a> {
    /// Per-tick time-series entries (kind [`KIND_TIME_SERIES`]).
    pub time_series: &'a [TimeSeriesData],
    /// Trend summary (kind [`KIND_TREND`]).
    pub trend: &'a TrendData,
    /// Aggregate summary (kind [`KIND_AGGREGATE`]).
    pub aggregate: &'a AggregateData,
}

impl<'a> StoreRecords<'a> {
    /// Builds the store-record view over `metrics`, mirroring `WriteToStore`.
    #[must_use]
    pub fn from_metrics(metrics: &'a ComputedMetrics) -> Self {
        Self {
            time_series: &metrics.time_series,
            trend: &metrics.trend,
            aggregate: &metrics.aggregate,
        }
    }
}

/// Extracts the per-tick sentiment dimension from stored `time_series` records.
///
/// Mirrors Go `extractStoreTimeSeries`: builds the tick axis and a single
/// `"sentiment"` dimension of per-tick scores (as `f64`). Returns `None` when no
/// time-series records are present, matching Go's `(nil, nil)`.
#[must_use]
pub fn extract_store_time_series(
    time_series: &[TimeSeriesData],
) -> Option<(Vec<i64>, Vec<(String, Vec<f64>)>)> {
    if time_series.is_empty() {
        return None;
    }

    let ticks: Vec<i64> = time_series.iter().map(|ts| ts.tick).collect();
    let sentiments: Vec<f64> = time_series.iter().map(|ts| f64::from(ts.sentiment)).collect();

    Some((ticks, vec![(DIM_SENTIMENT.to_string(), sentiments)]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ComputedMetrics;

    #[test]
    fn kind_constants_are_stable() {
        assert_eq!(KIND_TIME_SERIES, "time_series");
        assert_eq!(KIND_TREND, "trend");
        assert_eq!(KIND_AGGREGATE, "aggregate");
    }

    #[test]
    fn store_records_view_matches_metrics() {
        let metrics = ComputedMetrics {
            time_series: vec![TimeSeriesData { tick: 7, ..Default::default() }],
            aggregate: AggregateData { total_ticks: 1, ..Default::default() },
            ..Default::default()
        };
        let records = StoreRecords::from_metrics(&metrics);
        assert_eq!(records.time_series.len(), 1);
        assert_eq!(records.time_series[0].tick, 7);
        assert_eq!(records.aggregate.total_ticks, 1);
    }

    #[test]
    fn extract_store_time_series_empty() {
        assert!(extract_store_time_series(&[]).is_none());
    }

    #[test]
    fn extract_store_time_series_dimension() {
        let ts = vec![
            TimeSeriesData { tick: 0, sentiment: 0.8, ..Default::default() },
            TimeSeriesData { tick: 1, sentiment: 0.3, ..Default::default() },
        ];
        let (ticks, dims) = extract_store_time_series(&ts).expect("non-empty");
        assert_eq!(ticks, vec![0, 1]);
        assert_eq!(dims.len(), 1);
        assert_eq!(dims[0].0, "sentiment");
        assert!((dims[0].1[0] - 0.8).abs() < 1e-6);
        assert!((dims[0].1[1] - 0.3).abs() < 1e-6);
    }
}

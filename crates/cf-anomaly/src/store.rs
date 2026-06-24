//! Structured store record-kind constants and the store-write payload
//! mapping (the record set derived from [`ComputedMetrics`]). The actual
//! reader/writer I/O belongs to the `cf-analyze` store layer; see the crate
//! todos.

use crate::model::{
    AggregateData, ComputedMetrics, ExternalAnomaly, ExternalSummary, Record, TimeSeriesEntry,
};

/// Record kind: per-tick `TimeSeriesEntry` records (sorted by tick).
pub const KIND_TIME_SERIES: &str = "time_series";
/// Record kind: per-anomaly `Record` entries (sorted by Z-score desc).
pub const KIND_ANOMALY_RECORD: &str = "anomaly_record";
/// Record kind: the single `AggregateData` record.
pub const KIND_AGGREGATE: &str = "aggregate";
/// Record kind: cross-analyzer `ExternalAnomaly` records.
pub const KIND_EXTERNAL_ANOMALY: &str = "external_anomaly";
/// Record kind: cross-analyzer `ExternalSummary` records.
pub const KIND_EXTERNAL_SUMMARY: &str = "external_summary";

/// The full set of store records derived from computed metrics: the
/// `time_series` slice, the `anomaly_record` slice, and the single
/// `aggregate` record. Borrowing keeps this allocation-free; a store-writer
/// adapter (in `cf-analyze`, once wired) iterates these and writes each kind
/// through the byte-identical codec.
pub struct StoreRecords<'a> {
    /// Per-tick time-series entries (kind [`KIND_TIME_SERIES`]).
    pub time_series: &'a [TimeSeriesEntry],
    /// Per-anomaly records (kind [`KIND_ANOMALY_RECORD`]).
    pub anomalies: &'a [Record],
    /// Aggregate summary (kind [`KIND_AGGREGATE`]).
    pub aggregate: &'a AggregateData,
}

impl<'a> StoreRecords<'a> {
    /// Builds the store-record view over `metrics`.
    #[must_use]
    pub fn from_metrics(metrics: &'a ComputedMetrics) -> Self {
        Self {
            time_series: &metrics.time_series,
            anomalies: &metrics.anomalies,
            aggregate: &metrics.aggregate,
        }
    }
}

/// The enrichment record set written after cross-analyzer detection: the
/// `external_anomaly` and `external_summary` slices.
pub struct EnrichmentRecords<'a> {
    /// Cross-analyzer anomalies (kind [`KIND_EXTERNAL_ANOMALY`]).
    pub external_anomalies: &'a [ExternalAnomaly],
    /// Cross-analyzer summaries (kind [`KIND_EXTERNAL_SUMMARY`]).
    pub external_summaries: &'a [ExternalSummary],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_records_view_matches_metrics() {
        let metrics = ComputedMetrics {
            time_series: vec![TimeSeriesEntry {
                tick: 7,
                ..Default::default()
            }],
            anomalies: vec![Record {
                tick: 7,
                ..Default::default()
            }],
            aggregate: AggregateData {
                total_ticks: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let records = StoreRecords::from_metrics(&metrics);
        assert_eq!(records.time_series.len(), 1);
        assert_eq!(records.time_series[0].tick, 7);
        assert_eq!(records.anomalies.len(), 1);
        assert_eq!(records.aggregate.total_ticks, 1);
    }

    #[test]
    fn kind_constants_are_stable() {
        assert_eq!(KIND_TIME_SERIES, "time_series");
        assert_eq!(KIND_ANOMALY_RECORD, "anomaly_record");
        assert_eq!(KIND_AGGREGATE, "aggregate");
        assert_eq!(KIND_EXTERNAL_ANOMALY, "external_anomaly");
        assert_eq!(KIND_EXTERNAL_SUMMARY, "external_summary");
    }
}

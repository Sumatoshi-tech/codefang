//! Store record kinds and the per-tick / aggregate record set.
//!
//! Port of `internal/analyzers/quality/store_writer.go` and the read side of
//! `store_reader.go`. The Go `WriteToStore` streams two record kinds:
//!
//! * `"time_series"` — one [`crate::metrics::TimeSeriesEntry`] per tick, sorted.
//! * `"aggregate"` — a single [`crate::metrics::AggregateData`].
//!
//! The actual store I/O (begin/write/close, kind discovery) lives in
//! `cf-analyze`'s report store; this module produces the record *payloads* via
//! [`crate::serialize`] and names the kinds. Wiring `WriteToStore` /
//! `GenerateStoreSections` to the concrete `cf-analyze` `ReportWriter` /
//! `ReportReader` is deferred (see crate-level todos).

use crate::metrics::ComputedMetrics;

/// Store kind: per-tick time-series records.
pub const KIND_TIME_SERIES: &str = "time_series";
/// Store kind: the single aggregate record.
pub const KIND_AGGREGATE: &str = "aggregate";

/// Plot-section titles emitted by `GenerateStoreSections` (Go `store_reader.go`).
///
/// These are asserted byte-for-byte by the Go store round-trip tests; the charts
/// themselves are non-binding cosmetic output (DESIGN §2.7).
pub const SECTION_TITLE_COMPLEXITY: &str = "Cyclomatic Complexity Over Time";
/// Section title for the Halstead chart.
pub const SECTION_TITLE_HALSTEAD: &str = "Halstead Volume Over Time";
/// Section title for the summary stat grid.
pub const SECTION_TITLE_SUMMARY: &str = "Code Quality Summary";

/// Returns the JSON-record payloads to write to the store, in Go write order.
///
/// The first element is the `time_series` kind (a JSON array of entries); the
/// second is the `aggregate` kind (a single object). Each payload is produced by
/// the Go-compatible encoder so a round-trip is byte-stable.
#[must_use]
pub fn store_records(metrics: &ComputedMetrics) -> Vec<(&'static str, ComputedMetricsView<'_>)> {
    vec![
        (KIND_TIME_SERIES, ComputedMetricsView::TimeSeries(metrics)),
        (KIND_AGGREGATE, ComputedMetricsView::Aggregate(metrics)),
    ]
}

/// A view selecting which part of [`ComputedMetrics`] a store kind serializes.
#[derive(Debug, Clone, Copy)]
pub enum ComputedMetricsView<'a> {
    /// The `time_series` slice.
    TimeSeries(&'a ComputedMetrics),
    /// The `aggregate` object.
    Aggregate(&'a ComputedMetrics),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::ComputedMetrics;

    #[test]
    fn store_records_order_and_kinds() {
        let m = ComputedMetrics::default();
        let recs = store_records(&m);
        assert_eq!(recs[0].0, KIND_TIME_SERIES);
        assert_eq!(recs[1].0, KIND_AGGREGATE);
    }

    #[test]
    fn section_titles_match_go() {
        assert_eq!(SECTION_TITLE_COMPLEXITY, "Cyclomatic Complexity Over Time");
        assert_eq!(SECTION_TITLE_HALSTEAD, "Halstead Volume Over Time");
        assert_eq!(SECTION_TITLE_SUMMARY, "Code Quality Summary");
    }
}

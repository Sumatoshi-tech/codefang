//! RED (Rate, Errors, Duration) request metrics.
//!
//! Port of `internal/observability/metrics.go`. Instrument names, units,
//! descriptions, attribute keys, and the histogram bucket boundaries match the
//! Go source exactly so dashboards/alerts built against the Go binary keep
//! working unchanged.

use std::time::Duration;

use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
use opentelemetry::KeyValue;

use crate::metric_builder::{build_metrics, MetricBuildError};

// Instrument names (Go consts).
const METRIC_REQUESTS_TOTAL: &str = "codefang.requests.total";
const METRIC_REQUEST_DURATION: &str = "codefang.request.duration.seconds";
const METRIC_ERRORS_TOTAL: &str = "codefang.errors.total";
const METRIC_INFLIGHT_REQUESTS: &str = "codefang.inflight.requests";

// Attribute keys (Go consts).
const ATTR_OP: &str = "op";
const ATTR_STATUS: &str = "status";

/// Status value indicating an errored request (Go `statusError`).
const STATUS_ERROR: &str = "error";

/// Histogram bucket boundaries covering 10ms to 600s for analysis workloads.
///
/// Matches Go `durationBucketBoundaries` exactly. Long-running history pipelines
/// can span minutes, hence the buckets out to 600s.
pub const DURATION_BUCKET_BOUNDARIES: [f64; 14] =
    [0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0];

/// OTel instruments for Rate, Error, Duration metrics (Go `REDMetrics`).
pub struct RedMetrics {
    requests_total: Counter<u64>,
    request_duration: Histogram<f64>,
    errors_total: Counter<u64>,
    inflight_requests: UpDownCounter<i64>,
}

impl RedMetrics {
    /// Creates RED metric instruments from the given meter (Go `NewREDMetrics`).
    ///
    /// # Errors
    ///
    /// Returns the first instrument-build error, mirroring Go's behavior.
    pub fn new(meter: &Meter) -> Result<Self, MetricBuildError> {
        build_metrics(meter, |b| RedMetrics {
            requests_total: b
                .meter
                .u64_counter(METRIC_REQUESTS_TOTAL)
                .with_description("Total number of requests")
                .with_unit("{request}")
                .init(),
            request_duration: b
                .meter
                .f64_histogram(METRIC_REQUEST_DURATION)
                .with_description("Request duration in seconds")
                .with_unit("s")
                .with_boundaries(DURATION_BUCKET_BOUNDARIES.to_vec())
                .init(),
            errors_total: b
                .meter
                .u64_counter(METRIC_ERRORS_TOTAL)
                .with_description("Total number of errors")
                .with_unit("{error}")
                .init(),
            inflight_requests: b
                .meter
                .i64_up_down_counter(METRIC_INFLIGHT_REQUESTS)
                .with_description("Number of in-flight requests")
                .with_unit("{request}")
                .init(),
        })
    }

    /// Records a completed request with its operation, status, and duration.
    ///
    /// Port of Go `RecordRequest`. When `status == "error"` an additional
    /// `errors.total` increment is emitted carrying only the `op` attribute
    /// (matching Go's attribute set exactly).
    pub fn record_request(&self, op: &str, status: &str, duration: Duration) {
        let attrs = [
            KeyValue::new(ATTR_OP, op.to_string()),
            KeyValue::new(ATTR_STATUS, status.to_string()),
        ];

        self.requests_total.add(1, &attrs);
        self.request_duration.record(duration.as_secs_f64(), &attrs);

        if status == STATUS_ERROR {
            self.errors_total
                .add(1, &[KeyValue::new(ATTR_OP, op.to_string())]);
        }
    }

    /// Increments the in-flight gauge and returns a guard that decrements it.
    ///
    /// Port of Go `TrackInflight`, which returns a `func()`. The Rust analogue
    /// is an RAII [`InflightGuard`]; dropping it (or calling [`InflightGuard::done`])
    /// performs the `-1`.
    #[must_use]
    pub fn track_inflight(&self, op: &str) -> InflightGuard<'_> {
        let attr = KeyValue::new(ATTR_OP, op.to_string());
        self.inflight_requests.add(1, std::slice::from_ref(&attr));
        InflightGuard {
            inflight: &self.inflight_requests,
            attr,
            done: false,
        }
    }
}

/// RAII guard returned by [`RedMetrics::track_inflight`].
///
/// Decrements the in-flight up-down counter exactly once, either explicitly via
/// [`InflightGuard::done`] or on drop. Mirrors the closure returned by Go's
/// `TrackInflight`.
pub struct InflightGuard<'a> {
    inflight: &'a UpDownCounter<i64>,
    attr: KeyValue,
    done: bool,
}

impl InflightGuard<'_> {
    /// Decrements the in-flight counter (equivalent to invoking the Go closure).
    pub fn done(mut self) {
        self.decrement();
    }

    fn decrement(&mut self) {
        if !self.done {
            self.inflight.add(-1, std::slice::from_ref(&self.attr));
            self.done = true;
        }
    }
}

impl Drop for InflightGuard<'_> {
    fn drop(&mut self) {
        self.decrement();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_boundaries_match_go() {
        // Port of the boundary assertion in Go
        // `TestREDMetrics_HistogramBuckets_Extended`.
        let expected = [
            0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
        ];
        assert_eq!(DURATION_BUCKET_BOUNDARIES, expected);
    }

    #[test]
    fn metric_names_match_go() {
        assert_eq!(METRIC_REQUESTS_TOTAL, "codefang.requests.total");
        assert_eq!(METRIC_REQUEST_DURATION, "codefang.request.duration.seconds");
        assert_eq!(METRIC_ERRORS_TOTAL, "codefang.errors.total");
        assert_eq!(METRIC_INFLIGHT_REQUESTS, "codefang.inflight.requests");
    }

    /// Port of Go `TestNewREDMetrics_WithNilMeter` (no-op meter does not panic).
    #[test]
    fn new_with_noop_meter_records_without_panic() {
        use opentelemetry::metrics::MeterProvider;
        let meter = opentelemetry::metrics::noop::NoopMeterProvider::new().meter("test");
        let red = RedMetrics::new(&meter).expect("noop meter never errors");
        red.record_request("test", "ok", Duration::from_millis(1));
    }

    /// Port of Go `TestREDMetrics_TrackInflight` (guard increments then decrements).
    #[test]
    fn track_inflight_guard_runs() {
        use opentelemetry::metrics::MeterProvider;
        let meter = opentelemetry::metrics::noop::NoopMeterProvider::new().meter("test");
        let red = RedMetrics::new(&meter).unwrap();
        let guard = red.track_inflight("parse");
        guard.done();
    }
}

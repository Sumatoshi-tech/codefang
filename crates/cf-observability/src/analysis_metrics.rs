//! Analysis-specific metrics (commits, chunks, cache hits/misses).
//!
//! Instrument names, units, descriptions, and the `cache` attribute values
//! (`"blob"` / `"diff"`) are part of the telemetry contract — dashboards and
//! alerts key on them, so they must not change.

use std::time::Duration;

use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::KeyValue;

use crate::metric_builder::{build_metrics, MetricBuildError};

// Instrument names (telemetry contract).
const METRIC_COMMITS_TOTAL: &str = "codefang.analysis.commits.total";
const METRIC_CHUNKS_TOTAL: &str = "codefang.analysis.chunks.total";
const METRIC_CHUNK_DURATION: &str = "codefang.analysis.chunk.duration.seconds";
const METRIC_CACHE_HITS_TOTAL: &str = "codefang.analysis.cache.hits.total";
const METRIC_CACHE_MISSES_TOTAL: &str = "codefang.analysis.cache.misses.total";

/// Attribute key partitioning cache metrics by cache type.
const ATTR_CACHE: &str = "cache";

/// OTel instruments for analysis-specific metrics.
pub struct AnalysisMetrics {
    commits_total: Counter<u64>,
    chunks_total: Counter<u64>,
    chunk_duration: Histogram<f64>,
    cache_hits: Counter<u64>,
    cache_misses: Counter<u64>,
}

/// Statistics for a single streaming run, decoupled from framework types.
#[derive(Debug, Clone, Default)]
pub struct AnalysisStats {
    /// Number of commits analyzed.
    pub commits: i64,
    /// Number of chunks processed.
    pub chunks: i32,
    /// Per-chunk processing durations (one histogram observation each).
    pub chunk_durations: Vec<Duration>,
    /// Blob-cache hits.
    pub blob_cache_hits: i64,
    /// Blob-cache misses.
    pub blob_cache_misses: i64,
    /// Diff-cache hits.
    pub diff_cache_hits: i64,
    /// Diff-cache misses.
    pub diff_cache_misses: i64,
}

impl AnalysisMetrics {
    /// Creates the analysis metric instruments.
    ///
    /// # Errors
    ///
    /// Returns the first instrument-build error.
    pub fn new(meter: &Meter) -> Result<Self, MetricBuildError> {
        build_metrics(meter, |b| Self {
            commits_total: b
                .meter
                .u64_counter(METRIC_COMMITS_TOTAL)
                .with_description("Total commits analyzed")
                .with_unit("{commit}")
                .init(),
            chunks_total: b
                .meter
                .u64_counter(METRIC_CHUNKS_TOTAL)
                .with_description("Total chunks processed")
                .with_unit("{chunk}")
                .init(),
            chunk_duration: b
                .meter
                .f64_histogram(METRIC_CHUNK_DURATION)
                .with_description("Per-chunk processing duration in seconds")
                // Boundary advisory deferred to an SDK View built from
                // [`crate::metrics::DURATION_BUCKET_BOUNDARIES`] (see the note
                // in metrics.rs; otel-rust 0.24 lacks `with_boundaries`).
                .with_unit("s")
                .init(),
            cache_hits: b
                .meter
                .u64_counter(METRIC_CACHE_HITS_TOTAL)
                .with_description("Cache hits by type")
                .with_unit("{hit}")
                .init(),
            cache_misses: b
                .meter
                .u64_counter(METRIC_CACHE_MISSES_TOTAL)
                .with_description("Cache misses by type")
                .with_unit("{miss}")
                .init(),
        })
    }

    /// Records analysis statistics for a completed streaming run.
    ///
    /// Emits commits/chunks as counter totals, one histogram observation per
    /// chunk duration, and blob/diff cache hit/miss counters tagged with
    /// `cache=blob` / `cache=diff`. For callers holding an optional recorder,
    /// use [`AnalysisMetrics::record_run_opt`].
    pub fn record_run(&self, stats: &AnalysisStats) {
        self.commits_total.add(saturating_u64(stats.commits), &[]);
        self.chunks_total
            .add(saturating_u64(i64::from(stats.chunks)), &[]);

        for d in &stats.chunk_durations {
            self.chunk_duration.record(d.as_secs_f64(), &[]);
        }

        let blob_attrs = [KeyValue::new(ATTR_CACHE, "blob")];
        self.cache_hits
            .add(saturating_u64(stats.blob_cache_hits), &blob_attrs);
        self.cache_misses
            .add(saturating_u64(stats.blob_cache_misses), &blob_attrs);

        let diff_attrs = [KeyValue::new(ATTR_CACHE, "diff")];
        self.cache_hits
            .add(saturating_u64(stats.diff_cache_hits), &diff_attrs);
        self.cache_misses
            .add(saturating_u64(stats.diff_cache_misses), &diff_attrs);
    }

    /// `Option`-aware variant of [`AnalysisMetrics::record_run`]: `None` is a
    /// no-op, `Some(m)` delegates to [`AnalysisMetrics::record_run`].
    pub fn record_run_opt(this: Option<&Self>, stats: &AnalysisStats) {
        if let Some(m) = this {
            m.record_run(stats);
        }
    }
}

/// Converts a signed count to the unsigned counter delta the OTel API expects,
/// clamping negatives to 0 (these counters never legitimately go negative).
fn saturating_u64(v: i64) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::metrics::{noop::NoopMeterProvider, MeterProvider};

    fn noop_meter() -> Meter {
        NoopMeterProvider::new().meter("test")
    }

    /// Mirrors the reference suite's `TestNewAnalysisMetrics`.
    #[test]
    fn new_analysis_metrics() {
        let am = AnalysisMetrics::new(&noop_meter());
        assert!(am.is_ok());
    }

    /// Mirrors the reference suite's `TestAnalysisMetrics_RecordRun` (does not
    /// panic; 3 durations).
    #[test]
    fn record_run() {
        let am = AnalysisMetrics::new(&noop_meter()).unwrap();
        am.record_run(&AnalysisStats {
            commits: 100,
            chunks: 5,
            chunk_durations: vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(3),
            ],
            blob_cache_hits: 50,
            blob_cache_misses: 10,
            diff_cache_hits: 30,
            diff_cache_misses: 5,
        });
    }

    /// Mirrors the reference suite's `TestAnalysisMetrics_RecordRun_NilReceiver`.
    #[test]
    fn record_run_nil_receiver() {
        let none: Option<&AnalysisMetrics> = None;
        AnalysisMetrics::record_run_opt(
            none,
            &AnalysisStats {
                commits: 10,
                chunks: 1,
                ..Default::default()
            },
        );
    }

    #[test]
    fn metric_names_match_contract() {
        assert_eq!(METRIC_COMMITS_TOTAL, "codefang.analysis.commits.total");
        assert_eq!(METRIC_CHUNKS_TOTAL, "codefang.analysis.chunks.total");
        assert_eq!(
            METRIC_CHUNK_DURATION,
            "codefang.analysis.chunk.duration.seconds"
        );
        assert_eq!(
            METRIC_CACHE_HITS_TOTAL,
            "codefang.analysis.cache.hits.total"
        );
        assert_eq!(
            METRIC_CACHE_MISSES_TOTAL,
            "codefang.analysis.cache.misses.total"
        );
    }
}

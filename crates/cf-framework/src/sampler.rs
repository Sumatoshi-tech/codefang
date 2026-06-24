//! Periodic pipeline metrics sampler: configuration and deterministic
//! helpers.
//!
//! A [`PipelineSampler`] periodically logs memory + pipeline metrics during
//! chunk processing and captures `t0`/`t1` heap profiles. The live sampling
//! loop is a behavior-only path (no machine-output bytes); the deterministic
//! pieces live here:
//!
//! - [`SamplerConfig`] (the public config surface),
//! - the default [`SAMPLER_INTERVAL`],
//! - the once-only `t1` capture guard ([`PipelineSampler::capture_t1`]),
//! - the profile-path format string ([`profile_path`]).
//!
//! The injected logging/profile sink is the [`SamplerSink`] trait so a binary
//! can wire in the real observability backend; the default sink is a no-op.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::stage_metrics::StageMetrics;

/// Default polling interval for the pipeline sampler (2 seconds).
pub const SAMPLER_INTERVAL: Duration = Duration::from_secs(2);

/// Divisor for displaying values in thousands.
pub const KILO: i64 = 1000;

/// Configures the pipeline sampler. The logging/metrics handles are the
/// injected [`SamplerSink`] and a shared [`StageMetrics`].
#[derive(Debug, Clone, Default)]
pub struct SamplerConfig {
    /// Directory for heap-profile dumps. Empty disables profile capture.
    pub dump_dir: String,
    /// Index of the chunk being sampled (0-based; displayed +1).
    pub chunk_index: i64,
    /// The memory budget in bytes (for log context).
    pub mem_budget: i64,
    /// RSS (bytes) at which to capture the `t1` profile. 0 = disabled.
    pub profile_at_rss: i64,
    /// Polling interval. [`Duration::ZERO`] uses [`SAMPLER_INTERVAL`].
    pub interval: Duration,
}

/// Sink for sampler side effects (structured log lines + profile writes).
///
/// The default [`NoopSink`] discards everything; a binary supplies a real
/// implementation backed by `tracing` + a pprof writer for live sampling.
pub trait SamplerSink {
    /// Write the `t0`/`t1`/tick log+profile for a given label. `chunk_index` is
    /// 0-based.
    fn capture_profile(&self, dump_dir: &str, label: &str, chunk_index: i64);
}

/// A sink that records nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSink;

impl SamplerSink for NoopSink {
    fn capture_profile(&self, _dump_dir: &str, _label: &str, _chunk_index: i64) {}
}

/// Periodically captures pipeline metrics.
///
/// The `t1_captured` compare-and-swap guard guarantees that at most one `t1`
/// capture wins across the sampler loop and any concurrent
/// [`capture_t1`](Self::capture_t1) caller.
pub struct PipelineSampler {
    metrics: std::sync::Arc<StageMetrics>,
    interval: Duration,
    dump_dir: String,
    chunk_index: i64,
    #[allow(dead_code)]
    mem_budget: i64,
    #[allow(dead_code)]
    profile_at_rss: i64,
    t1_captured: AtomicBool,
}

impl PipelineSampler {
    /// Creates a sampler. The `interval` falls back to [`SAMPLER_INTERVAL`]
    /// when zero.
    #[must_use]
    pub fn new(cfg: SamplerConfig, metrics: std::sync::Arc<StageMetrics>) -> Self {
        let interval = if cfg.interval.is_zero() {
            SAMPLER_INTERVAL
        } else {
            cfg.interval
        };
        Self {
            metrics,
            interval,
            dump_dir: cfg.dump_dir,
            chunk_index: cfg.chunk_index,
            mem_budget: cfg.mem_budget,
            profile_at_rss: cfg.profile_at_rss,
            t1_captured: AtomicBool::new(false),
        }
    }

    /// The effective polling interval.
    #[must_use]
    pub const fn interval(&self) -> Duration {
        self.interval
    }

    /// Borrows the metrics this sampler reads.
    #[must_use]
    pub fn metrics(&self) -> &StageMetrics {
        &self.metrics
    }

    /// Captures the `t0` profile via the sink (no-op when `dump_dir` is
    /// empty); the start-of-loop pre-step.
    pub fn capture_t0<S: SamplerSink>(&self, sink: &S) {
        if self.dump_dir.is_empty() {
            return;
        }
        sink.capture_profile(&self.dump_dir, "t0", self.chunk_index);
    }

    /// Forces capture of the `t1` (peak) profile, exactly once. Safe to call
    /// concurrently with an automatic capture; the CAS guarantees a single
    /// winner.
    pub fn capture_t1<S: SamplerSink>(&self, sink: &S) {
        if self.dump_dir.is_empty() {
            return;
        }
        if self
            .t1_captured
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        sink.capture_profile(&self.dump_dir, "t1", self.chunk_index);
    }

    /// Tests whether the `t1` profile has been captured (for the auto-RSS path).
    #[must_use]
    pub fn t1_captured(&self) -> bool {
        self.t1_captured.load(Ordering::SeqCst)
    }
}

/// Formats a heap-profile path: `<dir>/heap_<label>_chunk<index>.pb.gz`
/// (a stable on-disk naming scheme; pinned by tests).
#[must_use]
pub fn profile_path(dir: &str, label: &str, chunk_index: i64) -> String {
    format!("{dir}/heap_{label}_chunk{chunk_index}.pb.gz")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn profile_path_matches_reference_format() {
        assert_eq!(
            profile_path("/tmp/dumps", "t0", 3),
            "/tmp/dumps/heap_t0_chunk3.pb.gz"
        );
        assert_eq!(profile_path("/d", "t1", 0), "/d/heap_t1_chunk0.pb.gz");
    }

    #[test]
    fn interval_defaults_when_zero() {
        let s = PipelineSampler::new(SamplerConfig::default(), Arc::new(StageMetrics::new()));
        assert_eq!(s.interval(), SAMPLER_INTERVAL);
    }

    #[test]
    fn interval_honors_explicit() {
        let cfg = SamplerConfig {
            interval: Duration::from_millis(500),
            ..SamplerConfig::default()
        };
        let s = PipelineSampler::new(cfg, Arc::new(StageMetrics::new()));
        assert_eq!(s.interval(), Duration::from_millis(500));
    }

    #[test]
    fn t1_capture_is_once_only() {
        struct CountingSink {
            count: std::sync::atomic::AtomicI64,
        }
        impl SamplerSink for CountingSink {
            fn capture_profile(&self, _d: &str, _l: &str, _c: i64) {
                self.count.fetch_add(1, Ordering::SeqCst);
            }
        }
        let cfg = SamplerConfig {
            dump_dir: "/tmp".to_string(),
            ..SamplerConfig::default()
        };
        let s = PipelineSampler::new(cfg, Arc::new(StageMetrics::new()));
        let sink = CountingSink {
            count: std::sync::atomic::AtomicI64::new(0),
        };
        s.capture_t1(&sink);
        s.capture_t1(&sink);
        s.capture_t1(&sink);
        assert_eq!(sink.count.load(Ordering::SeqCst), 1);
        assert!(s.t1_captured());
    }

    #[test]
    fn capture_disabled_when_dump_dir_empty() {
        struct PanicSink;
        impl SamplerSink for PanicSink {
            fn capture_profile(&self, _d: &str, _l: &str, _c: i64) {
                panic!("must not capture when dump_dir empty");
            }
        }
        let s = PipelineSampler::new(SamplerConfig::default(), Arc::new(StageMetrics::new()));
        s.capture_t0(&PanicSink);
        s.capture_t1(&PanicSink);
    }
}

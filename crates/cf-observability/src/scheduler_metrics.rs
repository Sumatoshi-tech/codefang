//! Runtime scheduler metrics, exposed as OTel gauges.
//!
//! # Runtime-source caveat (behavioral, not byte-binding)
//!
//! These instruments were defined for a runtime with green-thread scheduler
//! metrics (live goroutine/thread counts); this runtime has no equivalent
//! stable metrics surface, and the values are non-deterministic gauges that
//! are NOT part of any machine report (DESIGN §3). What is preserved is the
//! *telemetry contract*: the same three instrument names/units/descriptions
//! are registered, an observer callback is wired in, and registration
//! succeeds with a no-op meter. The numeric sampler is pluggable via
//! [`RuntimeSampler`] so a richer source (e.g. tokio runtime metrics) can be
//! supplied without changing the instrument surface. See crate-level todos.

use std::sync::Arc;

use opentelemetry::metrics::{Meter, MetricsError, ObservableGauge};

use crate::metric_builder::MetricBuildError;

// Instrument names (telemetry contract; the goroutine wording is the
// established dashboard surface).
const METRIC_GOROUTINES: &str = "codefang.runtime.goroutines";
const METRIC_THREADS: &str = "codefang.runtime.threads";
const METRIC_GOROUTINES_CREATED: &str = "codefang.runtime.goroutines.created";

/// Supplies the current scheduler sample values for the observable callback.
///
/// `(goroutines, threads, goroutines_created)`. The default
/// [`NullRuntimeSampler`] reports zeros, mirroring a runtime with no live
/// concurrency signal; production callers may inject a real sampler.
pub trait RuntimeSampler: Send + Sync + 'static {
    /// Returns the live goroutine-equivalent count.
    fn goroutines(&self) -> i64 {
        0
    }
    /// Returns the live OS-thread count.
    fn threads(&self) -> i64 {
        0
    }
    /// Returns the cumulative goroutine-equivalent creation count.
    fn goroutines_created(&self) -> i64 {
        0
    }
}

/// Zero-reporting sampler used when no runtime metric source is available.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullRuntimeSampler;

impl RuntimeSampler for NullRuntimeSampler {}

/// Scheduler metrics exposed as OTel instruments. Holding the gauges keeps
/// the registered callback alive for the meter's lifetime.
pub struct SchedulerMetrics {
    _goroutines: ObservableGauge<i64>,
    _threads: ObservableGauge<i64>,
    _goroutines_created: ObservableGauge<i64>,
}

impl SchedulerMetrics {
    /// Creates the scheduler instruments backed by [`NullRuntimeSampler`].
    ///
    /// The meter's periodic reader invokes the registered callback
    /// automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if instrument or callback registration fails.
    pub fn new(meter: &Meter) -> Result<Self, MetricBuildError> {
        Self::with_sampler(meter, NullRuntimeSampler)
    }

    /// Creates the scheduler instruments backed by a custom [`RuntimeSampler`].
    ///
    /// # Errors
    ///
    /// Returns an error if instrument or callback registration fails.
    pub fn with_sampler<S: RuntimeSampler>(
        meter: &Meter,
        sampler: S,
    ) -> Result<Self, MetricBuildError> {
        let goroutines = meter
            .i64_observable_gauge(METRIC_GOROUTINES)
            .with_description("Current number of live goroutines")
            .with_unit("{goroutine}")
            .init();
        let threads = meter
            .i64_observable_gauge(METRIC_THREADS)
            .with_description("Current number of OS threads created by the Go runtime")
            .with_unit("{thread}")
            .init();
        let goroutines_created = meter
            .i64_observable_gauge(METRIC_GOROUTINES_CREATED)
            .with_description("Total goroutines created since process start")
            .with_unit("{goroutine}")
            .init();

        let sampler = Arc::new(sampler);
        let g = goroutines.clone();
        let t = threads.clone();
        let gc = goroutines_created.clone();

        // Single callback observing all three instruments.
        meter
            .register_callback(&[g.as_any(), t.as_any(), gc.as_any()], move |obs| {
                obs.observe_i64(&g, sampler.goroutines(), &[]);
                obs.observe_i64(&t, sampler.threads(), &[]);
                obs.observe_i64(&gc, sampler.goroutines_created(), &[]);
            })
            .map_err(|e: MetricsError| {
                MetricBuildError::new("register scheduler metrics callback", e)
            })?;

        Ok(Self {
            _goroutines: goroutines,
            _threads: threads,
            _goroutines_created: goroutines_created,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::metrics::{noop::NoopMeterProvider, MeterProvider};

    /// Mirrors the reference suite's `TestNewSchedulerMetrics_NoopMeter`.
    #[test]
    fn new_scheduler_metrics_noop_meter() {
        let meter = NoopMeterProvider::new().meter("test");
        let sm = SchedulerMetrics::new(&meter);
        assert!(sm.is_ok());
    }

    #[test]
    fn metric_names_match_contract() {
        assert_eq!(METRIC_GOROUTINES, "codefang.runtime.goroutines");
        assert_eq!(METRIC_THREADS, "codefang.runtime.threads");
        assert_eq!(
            METRIC_GOROUTINES_CREATED,
            "codefang.runtime.goroutines.created"
        );
    }
}

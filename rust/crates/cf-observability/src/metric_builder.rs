//! Batch metric-instrument construction with first-error-wins accumulation.
//!
//! Port of `internal/observability/metric_builder.go`. The Go code wraps OTel
//! instrument constructors (which return `(T, error)`) in a builder that records
//! the first error so a whole metrics struct can be built with a single error
//! check. The Rust OTel API constructors are infallible (`build()` returns the
//! instrument directly), but downstream callers and ported tests still exercise
//! the first-error-wins accumulation contract, so it is reproduced exactly.

use std::fmt;

use opentelemetry::metrics::Meter;

/// Error produced when an instrument fails to build.
///
/// Mirrors Go's wrapped error `fmt.Errorf("create %s: %w", name, err)`: the
/// `Display` form is `create <name>: <source>` and [`std::error::Error::source`]
/// returns the wrapped cause (so `ErrorIs`-style downcasting works).
#[derive(Debug)]
pub struct MetricBuildError {
    name: String,
    source: Box<dyn std::error::Error + Send + Sync + 'static>,
}

impl MetricBuildError {
    /// Creates a build error wrapping `source`, attributed to instrument `name`.
    pub fn new(
        name: impl Into<String>,
        source: impl Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    ) -> Self {
        MetricBuildError {
            name: name.into(),
            source: source.into(),
        }
    }

    /// The instrument name the error is attributed to.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for MetricBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Matches Go's "create <name>: <wrapped>".
        write!(f, "create {}: {}", self.name, self.source)
    }
}

impl std::error::Error for MetricBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// Accumulates instrument-creation errors, enabling batch construction with a
/// single error check (Go `metricBuilder`).
pub struct MetricBuilder<'m> {
    /// Meter that instruments are created from.
    pub meter: &'m Meter,
    err: Option<MetricBuildError>,
}

impl<'m> MetricBuilder<'m> {
    /// Creates a builder for the given meter (Go `newMetricBuilder`).
    #[must_use]
    pub fn new(meter: &'m Meter) -> Self {
        MetricBuilder { meter, err: None }
    }

    /// Builds an instrument via `f`, recording any error against `name`.
    ///
    /// Port of the generic Go `createMetric[T]`. `f` returns `Result<T, E>`;
    /// on `Err` the first error is retained (later errors ignored) and the
    /// instrument value is taken from `default_on_err`.
    pub fn create_metric<T, E>(
        &mut self,
        name: &str,
        default_on_err: impl FnOnce() -> T,
        f: impl FnOnce(&Meter) -> Result<T, E>,
    ) -> T
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        match f(self.meter) {
            Ok(v) => v,
            Err(e) => {
                self.set_err(name, e);
                default_on_err()
            }
        }
    }

    /// Records the first instrument-creation error (Go `setErr`).
    ///
    /// Only the first error is retained; subsequent calls are ignored.
    pub fn set_err<E>(&mut self, name: &str, err: E)
    where
        E: Into<Box<dyn std::error::Error + Send + Sync + 'static>>,
    {
        if self.err.is_none() {
            self.err = Some(MetricBuildError::new(name.to_string(), err));
        }
    }

    /// Returns the accumulated error, if any (read-only, mirrors Go `b.err`).
    #[must_use]
    pub fn err(&self) -> Option<&MetricBuildError> {
        self.err.as_ref()
    }

    /// Consumes the builder, returning `Err` if any instrument failed.
    ///
    /// Equivalent to the error check inside Go `buildMetrics`.
    pub fn finish<T>(self, result: T) -> Result<T, MetricBuildError> {
        match self.err {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }
}

/// Constructs a metrics struct by delegating instrument creation to `f`.
///
/// Port of the generic Go `buildMetrics[T]`: creates a builder, runs `f`, and
/// returns the struct only if no instrument errored.
pub fn build_metrics<T, F>(meter: &Meter, f: F) -> Result<T, MetricBuildError>
where
    F: FnOnce(&mut MetricBuilder<'_>) -> T,
{
    let mut b = MetricBuilder::new(meter);
    let result = f(&mut b);
    b.finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::metrics::MeterProvider;

    fn test_meter() -> Meter {
        // A no-op MeterProvider yields infallible instruments — exactly Go's
        // noopmetric.NewMeterProvider().Meter("test").
        opentelemetry::metrics::noop::NoopMeterProvider::new().meter("test")
    }

    #[derive(Debug)]
    struct TestErr(&'static str);
    impl fmt::Display for TestErr {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for TestErr {}

    /// Port of Go `TestCreateMetric_Counter` — successful build, no error.
    #[test]
    fn create_metric_counter() {
        let meter = test_meter();
        let mut b = MetricBuilder::new(&meter);

        let _c = b.create_metric::<_, TestErr>(
            "test.metric",
            || meter.u64_counter("fallback").init(),
            |m| Ok(m.u64_counter("test.metric").with_description("A test metric").with_unit("{item}").init()),
        );

        assert!(b.err().is_none());
    }

    /// Port of Go `TestCreateMetric_ErrorAccumulation_CapturesFirst`.
    #[test]
    fn error_accumulation_captures_first() {
        let meter = test_meter();
        let mut b = MetricBuilder::new(&meter);

        b.set_err("first.metric", TestErr("test: creation failed"));

        let e = b.err().expect("error should be recorded");
        assert!(e.to_string().contains("first.metric"));
        assert!(e.to_string().contains("test: creation failed"));
    }

    /// Port of Go `TestCreateMetric_ErrorAccumulation_IgnoresSubsequent`.
    #[test]
    fn error_accumulation_ignores_subsequent() {
        let meter = test_meter();
        let mut b = MetricBuilder::new(&meter);

        b.set_err("first.metric", TestErr("test: creation failed"));
        b.set_err("second.metric", TestErr("second error"));

        let e = b.err().expect("error should be recorded");
        // Only the first error is retained.
        assert!(e.name() == "first.metric");
        assert!(e.to_string().contains("test: creation failed"));
        assert!(!e.to_string().contains("second error"));
    }

    /// Port of Go `TestBuildMetrics_Success`.
    #[test]
    fn build_metrics_success() {
        struct M {
            _counter: opentelemetry::metrics::Counter<u64>,
        }
        let meter = test_meter();
        let result = build_metrics(&meter, |b| M {
            _counter: b.create_metric::<_, TestErr>(
                "test.metric",
                || b.meter.u64_counter("fallback").init(),
                |m| Ok(m.u64_counter("test.metric").init()),
            ),
        });
        assert!(result.is_ok());
    }

    /// Port of Go `TestBuildMetrics_PropagatesError`.
    #[test]
    fn build_metrics_propagates_error() {
        struct Empty;
        let meter = test_meter();
        let result: Result<Empty, _> = build_metrics(&meter, |b| {
            b.set_err("forced.failure", TestErr("test: creation failed"));
            Empty
        });
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("forced.failure"));
    }
}

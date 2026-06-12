//! Batch metric-instrument construction with first-error-wins accumulation.
//!
//! Wraps fallible instrument construction in a builder that records the first
//! error, so a whole metrics struct can be built with a single error check.
//! The current OTel-Rust API constructors are infallible, but downstream
//! callers and the metrics tests exercise the first-error-wins accumulation
//! contract, so the builder keeps it.

use opentelemetry::metrics::Meter;

/// Error produced when an instrument fails to build.
///
/// The `Display` form is `create <name>: <source>` (a stable operator-facing
/// wording) and [`std::error::Error::source`] returns the wrapped cause so
/// callers can downcast it.
#[derive(Debug, thiserror::Error)]
#[error("create {name}: {source}")]
pub struct MetricBuildError {
    name: String,
    #[source]
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

/// Accumulates instrument-creation errors, enabling batch construction with a
/// single error check.
pub struct MetricBuilder<'m> {
    /// Meter that instruments are created from.
    pub meter: &'m Meter,
    err: Option<MetricBuildError>,
}

impl<'m> MetricBuilder<'m> {
    /// Creates a builder for the given meter.
    #[must_use]
    pub fn new(meter: &'m Meter) -> Self {
        MetricBuilder { meter, err: None }
    }

    /// Builds an instrument via `f`, recording any error against `name`.
    ///
    /// `f` returns `Result<T, E>`; on `Err` the first error is retained (later
    /// errors ignored) and the instrument value is taken from `default_on_err`.
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

    /// Records the first instrument-creation error.
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

    /// Returns the accumulated error, if any.
    #[must_use]
    pub fn err(&self) -> Option<&MetricBuildError> {
        self.err.as_ref()
    }

    /// Consumes the builder, returning `Err` if any instrument failed.
    ///
    /// # Errors
    ///
    /// Returns the first recorded instrument-creation error.
    pub fn finish<T>(self, result: T) -> Result<T, MetricBuildError> {
        self.err.map_or(Ok(result), Err)
    }
}

/// Constructs a metrics struct by delegating instrument creation to `f`.
///
/// Creates a builder, runs `f`, and returns the struct only if no instrument
/// errored.
///
/// # Errors
///
/// Returns the first instrument-creation error recorded by the builder.
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
        // A no-op MeterProvider yields infallible instruments.
        opentelemetry::metrics::noop::NoopMeterProvider::new().meter("test")
    }

    #[derive(Debug)]
    struct TestErr(&'static str);
    impl std::fmt::Display for TestErr {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for TestErr {}

    /// Mirrors the reference suite's `TestCreateMetric_Counter` — successful build, no error.
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

    /// Mirrors the reference suite's `TestCreateMetric_ErrorAccumulation_CapturesFirst`.
    #[test]
    fn error_accumulation_captures_first() {
        let meter = test_meter();
        let mut b = MetricBuilder::new(&meter);

        b.set_err("first.metric", TestErr("test: creation failed"));

        let e = b.err().expect("error should be recorded");
        assert!(e.to_string().contains("first.metric"));
        assert!(e.to_string().contains("test: creation failed"));
    }

    /// Mirrors the reference suite's `TestCreateMetric_ErrorAccumulation_IgnoresSubsequent`.
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

    /// Mirrors the reference suite's `TestBuildMetrics_Success`.
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

    /// Mirrors the reference suite's `TestBuildMetrics_PropagatesError`.
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

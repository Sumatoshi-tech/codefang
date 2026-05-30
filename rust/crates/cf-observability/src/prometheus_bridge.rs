//! Prometheus `/metrics` exporter backed by an OTel `MeterProvider`.
//!
//! Port of `internal/observability/prometheus.go`. Builds a Prometheus registry,
//! attaches the OTel→Prometheus exporter as a reader on a `MeterProvider`, and
//! returns the pieces needed to serve the `/metrics` scrape endpoint.
//!
//! Go returns an `http.Handler`; the Rust analogue returns a
//! [`PrometheusScrape`] holding the registry, which [`crate::diagnostics`] serves
//! via hyper. Each call creates an independent registry to avoid collector
//! conflicts (matching Go's `prometheus.NewRegistry()` per call).

use opentelemetry::metrics::MetricsError;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use prometheus::{Registry, TextEncoder};

/// A built Prometheus scrape target: an independent registry plus the OTel
/// `MeterProvider` whose instruments it collects.
///
/// Hold both for the lifetime of the diagnostics server; dropping the provider
/// stops collection.
pub struct PrometheusScrape {
    registry: Registry,
    /// The MeterProvider wired to the Prometheus exporter. Kept alive so the
    /// exporter has a metrics source (mirrors Go attaching the exporter as a
    /// reader on a MeterProvider).
    pub meter_provider: SdkMeterProvider,
}

impl PrometheusScrape {
    /// Creates a Prometheus exporter backed by a fresh OTel `MeterProvider`
    /// (Go `PrometheusHandler`).
    ///
    /// # Errors
    ///
    /// Returns an error if the exporter cannot be created.
    pub fn new() -> Result<Self, MetricsError> {
        let registry = Registry::new();

        let exporter = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .build()?;

        // Attach the exporter as a reader so OTel instruments are collected.
        let meter_provider = SdkMeterProvider::builder().with_reader(exporter).build();

        Ok(PrometheusScrape {
            registry,
            meter_provider,
        })
    }

    /// Renders the current metrics in the Prometheus text exposition format,
    /// equivalent to what `promhttp.HandlerFor` serves on `GET /metrics`.
    ///
    /// The returned `(content_type, body)` pair uses the standard
    /// `text/plain; version=0.0.4` content type, matching the Go handler.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    pub fn render(&self) -> Result<(String, String), prometheus::Error> {
        let metric_families = self.registry.gather();
        let encoder = TextEncoder::new();
        let body = encoder.encode_to_string(&metric_families)?;
        let content_type = "text/plain; version=0.0.4; charset=utf-8".to_string();
        Ok((content_type, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Port of Go `TestPrometheusHandler_ServesMetrics` (handler builds; the
    /// exposition is text/plain). Network serving is covered by diagnostics tests.
    #[test]
    fn builds_and_renders_text_plain() {
        let scrape = PrometheusScrape::new().expect("exporter builds");
        let (content_type, _body) = scrape.render().expect("render succeeds");
        assert!(content_type.contains("text/plain"));
    }
}

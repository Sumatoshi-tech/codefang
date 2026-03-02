package observability

import (
	"fmt"
	"net/http"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promhttp"
	promexporter "go.opentelemetry.io/otel/exporters/prometheus"
	sdkmetric "go.opentelemetry.io/otel/sdk/metric"
)

// PrometheusHandler creates a Prometheus metrics exporter backed by an OTel
// MeterProvider and returns an [http.Handler] that serves the /metrics scrape
// endpoint. Each call creates an independent Prometheus registry to avoid
// collector conflicts when called multiple times.
func PrometheusHandler() (http.Handler, error) {
	registry := prometheus.NewRegistry()

	exporter, err := promexporter.New(
		promexporter.WithRegisterer(registry),
	)
	if err != nil {
		return nil, fmt.Errorf("create prometheus exporter: %w", err)
	}

	// Attach the exporter as a reader to a MeterProvider so OTel instruments
	// are collected. Without this the exporter has no metrics source.
	_ = sdkmetric.NewMeterProvider(sdkmetric.WithReader(exporter))

	return promhttp.HandlerFor(registry, promhttp.HandlerOpts{}), nil
}

package observability

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"net/http"

	"go.opentelemetry.io/otel/metric"
)

// DiagnosticsServer exposes health, readiness, and Prometheus metrics
// endpoints over HTTP for operational monitoring.
type DiagnosticsServer struct {
	server   *http.Server
	listener net.Listener
}

// NewDiagnosticsServer starts an HTTP server at addr with /healthz, /readyz,
// and /metrics endpoints. The meter is used to register scheduler metrics.
// Pass a nil meter to skip scheduler metrics registration.
func NewDiagnosticsServer(addr string, meter metric.Meter) (*DiagnosticsServer, error) {
	mux := http.NewServeMux()

	mux.Handle("/healthz", HealthHandler())
	mux.Handle("/readyz", ReadyHandler())

	metricsHandler, err := PrometheusHandler()
	if err != nil {
		return nil, fmt.Errorf("create prometheus handler: %w", err)
	}

	mux.Handle("/metrics", metricsHandler)

	if meter != nil {
		_, err = NewSchedulerMetrics(meter)
		if err != nil {
			return nil, fmt.Errorf("register scheduler metrics: %w", err)
		}
	}

	var lc net.ListenConfig

	listener, err := lc.Listen(context.Background(), "tcp", addr)
	if err != nil {
		return nil, fmt.Errorf("listen on %s: %w", addr, err)
	}

	srv := &http.Server{Handler: mux}

	go func() {
		serveErr := srv.Serve(listener)
		if serveErr != nil && !errors.Is(serveErr, http.ErrServerClosed) {
			slog.Warn("diagnostics server stopped", "error", serveErr)
		}
	}()

	return &DiagnosticsServer{server: srv, listener: listener}, nil
}

// Addr returns the address the server is listening on.
func (d *DiagnosticsServer) Addr() string {
	return d.listener.Addr().String()
}

// Close gracefully shuts down the diagnostics server.
func (d *DiagnosticsServer) Close() error {
	err := d.server.Shutdown(context.Background())
	if err != nil {
		return fmt.Errorf("shutdown diagnostics server: %w", err)
	}

	return nil
}

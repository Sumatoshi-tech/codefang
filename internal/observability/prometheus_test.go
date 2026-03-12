package observability_test

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
)

func TestPrometheusHandler_ServesMetrics(t *testing.T) {
	t.Parallel()

	handler, err := observability.PrometheusHandler()
	require.NoError(t, err)

	req := httptest.NewRequest(http.MethodGet, "/metrics", http.NoBody)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)
	// Prometheus exposition format uses text/plain with version parameter.
	assert.Contains(t, rec.Header().Get("Content-Type"), "text/plain")
}

func TestPrometheusHandler_ContainsTargetInfo(t *testing.T) {
	t.Parallel()

	handler, err := observability.PrometheusHandler()
	require.NoError(t, err)

	req := httptest.NewRequest(http.MethodGet, "/metrics", http.NoBody)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	// The OTel Prometheus exporter includes target_info with SDK metadata.
	body := rec.Body.String()
	assert.Contains(t, body, "target_info")
}

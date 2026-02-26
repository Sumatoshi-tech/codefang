package observability_test

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
)

func TestHealthHandler_ReturnsOK(t *testing.T) {
	t.Parallel()

	handler := observability.HealthHandler()

	req := httptest.NewRequest(http.MethodGet, "/healthz", http.NoBody)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)

	var body map[string]string

	err := json.Unmarshal(rec.Body.Bytes(), &body)
	require.NoError(t, err)
	assert.Equal(t, "ok", body["status"])
}

func TestHealthHandler_ContentTypeJSON(t *testing.T) {
	t.Parallel()

	handler := observability.HealthHandler()

	req := httptest.NewRequest(http.MethodGet, "/healthz", http.NoBody)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	assert.Equal(t, "application/json", rec.Header().Get("Content-Type"))
}

func TestReadyHandler_AllChecksPass(t *testing.T) {
	t.Parallel()

	passCheckA := func(_ context.Context) error { return nil }
	passCheckB := func(_ context.Context) error { return nil }
	handler := observability.ReadyHandler(passCheckA, passCheckB)

	req := httptest.NewRequest(http.MethodGet, "/readyz", http.NoBody)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)

	var body map[string]string

	err := json.Unmarshal(rec.Body.Bytes(), &body)
	require.NoError(t, err)
	assert.Equal(t, "ok", body["status"])
}

func TestReadyHandler_NoChecks(t *testing.T) {
	t.Parallel()

	handler := observability.ReadyHandler()

	req := httptest.NewRequest(http.MethodGet, "/readyz", http.NoBody)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	assert.Equal(t, http.StatusOK, rec.Code)
}

var errTestDBUnreachable = errors.New("db unreachable")

func TestReadyHandler_CheckFails(t *testing.T) {
	t.Parallel()

	errDB := errTestDBUnreachable
	failCheck := func(_ context.Context) error { return errDB }
	passCheck := func(_ context.Context) error { return nil }

	handler := observability.ReadyHandler(passCheck, failCheck)

	req := httptest.NewRequest(http.MethodGet, "/readyz", http.NoBody)
	rec := httptest.NewRecorder()

	handler.ServeHTTP(rec, req)

	assert.Equal(t, http.StatusServiceUnavailable, rec.Code)

	var body map[string]string

	err := json.Unmarshal(rec.Body.Bytes(), &body)
	require.NoError(t, err)
	assert.Equal(t, "unavailable", body["status"])
}

package observability_test

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/propagation"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

func TestHTTPMiddleware_CreatesSpan(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")

	handler := http.HandlerFunc(func(rw http.ResponseWriter, _ *http.Request) {
		rw.WriteHeader(http.StatusOK)
	})

	mw := observability.HTTPMiddleware(tracer, handler)

	req := httptest.NewRequest(http.MethodGet, "/v1/analyze", http.NoBody)
	rec := httptest.NewRecorder()

	mw.ServeHTTP(rec, req)

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)
	assert.Equal(t, "GET /v1/analyze", spans[0].Name)
}

func TestHTTPMiddleware_PropagatesContext(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")

	var handlerCalled bool

	handler := http.HandlerFunc(func(rw http.ResponseWriter, _ *http.Request) {
		handlerCalled = true

		rw.WriteHeader(http.StatusOK)
	})

	mw := observability.HTTPMiddleware(tracer, handler)

	req := httptest.NewRequest(http.MethodPost, "/v1/history", http.NoBody)
	rec := httptest.NewRecorder()

	mw.ServeHTTP(rec, req)

	// The handler should have been called with a span-bearing context.
	require.True(t, handlerCalled)

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)
	assert.Equal(t, "POST /v1/history", spans[0].Name)
}

func TestHTTPMiddleware_ExtractsTraceParent(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	// Register W3C propagator globally (same as Init does).
	otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator(
		propagation.TraceContext{},
		propagation.Baggage{},
	))

	tracer := tp.Tracer("test")

	// Create a known parent trace/span ID via traceparent header.
	parentTraceID := "0af7651916cd43dd8448eb211c80319c"
	parentSpanID := "00f067aa0ba902b7"
	traceparent := "00-" + parentTraceID + "-" + parentSpanID + "-01"

	handler := http.HandlerFunc(func(rw http.ResponseWriter, _ *http.Request) {
		rw.WriteHeader(http.StatusOK)
	})

	mw := observability.HTTPMiddleware(tracer, handler)

	req := httptest.NewRequest(http.MethodGet, "/v1/analyze", http.NoBody)
	req.Header.Set("Traceparent", traceparent)

	rec := httptest.NewRecorder()

	mw.ServeHTTP(rec, req)

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)

	// The span's parent should match the incoming traceparent.
	assert.Equal(t, parentTraceID, spans[0].SpanContext.TraceID().String())
	assert.Equal(t, parentSpanID, spans[0].Parent.SpanID().String())
}

func TestHTTPMiddleware_SetsStatusOnError(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")

	handler := http.HandlerFunc(func(rw http.ResponseWriter, _ *http.Request) {
		rw.WriteHeader(http.StatusInternalServerError)
	})

	mw := observability.HTTPMiddleware(tracer, handler)

	req := httptest.NewRequest(http.MethodGet, "/v1/score", http.NoBody)
	rec := httptest.NewRecorder()

	mw.ServeHTTP(rec, req)

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)

	assert.Equal(t, http.StatusInternalServerError, rec.Code)
}

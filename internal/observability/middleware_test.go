package observability_test

import (
	"bytes"
	"context"
	"errors"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/codes"
	"go.opentelemetry.io/otel/propagation"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
)

var (
	errConnectionRefused = errors.New("connection refused")
	errBadInput          = errors.New("bad input")
)

var discardLogger = slog.New(slog.NewTextHandler(bytes.NewBuffer(nil), nil))

func TestHTTPMiddleware_CreatesSpan(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")

	handler := http.HandlerFunc(func(rw http.ResponseWriter, _ *http.Request) {
		rw.WriteHeader(http.StatusOK)
	})

	mw := observability.HTTPMiddleware(tracer, discardLogger, handler)

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

	mw := observability.HTTPMiddleware(tracer, discardLogger, handler)

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

	mw := observability.HTTPMiddleware(tracer, discardLogger, handler)

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

func TestHTTPMiddleware_RecoversPanic(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")

	handler := http.HandlerFunc(func(_ http.ResponseWriter, _ *http.Request) {
		panic("unexpected nil pointer")
	})

	mw := observability.HTTPMiddleware(tracer, discardLogger, handler)

	req := httptest.NewRequest(http.MethodGet, "/v1/crash", http.NoBody)
	rec := httptest.NewRecorder()

	// Should not panic — middleware should recover.
	require.NotPanics(t, func() {
		mw.ServeHTTP(rec, req)
	})

	assert.Equal(t, http.StatusInternalServerError, rec.Code)

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)

	span := spans[0]

	// Verify error.type is "panic".
	var foundErrType bool

	for _, attr := range span.Attributes {
		if string(attr.Key) == "error.type" && attr.Value.AsString() == "panic" {
			foundErrType = true
		}
	}

	assert.True(t, foundErrType, "span should have error.type=panic attribute")

	// Verify stack trace event exists.
	var foundStackEvent bool

	for _, event := range span.Events {
		if event.Name == "panic.stack" {
			foundStackEvent = true
		}
	}

	assert.True(t, foundStackEvent, "span should have panic.stack event with stack trace")
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

	mw := observability.HTTPMiddleware(tracer, discardLogger, handler)

	req := httptest.NewRequest(http.MethodGet, "/v1/score", http.NoBody)
	rec := httptest.NewRecorder()

	mw.ServeHTTP(rec, req)

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)

	assert.Equal(t, http.StatusInternalServerError, rec.Code)
}

func TestRecordSpanError_SetsAttributes(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")
	_, span := tracer.Start(context.Background(), "test.op")

	testErr := errConnectionRefused

	observability.RecordSpanError(span, testErr, observability.ErrTypeDependencyUnavailable, observability.ErrSourceDependency)
	span.End()

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)

	recorded := spans[0]

	assert.Equal(t, codes.Error, recorded.Status.Code)
	assert.Equal(t, "connection refused", recorded.Status.Description)

	assertAttribute(t, recorded.Attributes, "error.type", observability.ErrTypeDependencyUnavailable)
	assertAttribute(t, recorded.Attributes, "error.source", observability.ErrSourceDependency)
}

func TestRecordSpanError_EmptySource(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")
	_, span := tracer.Start(context.Background(), "test.op")

	testErr := errBadInput

	observability.RecordSpanError(span, testErr, observability.ErrTypeValidation, "")
	span.End()

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)

	recorded := spans[0]

	assertAttribute(t, recorded.Attributes, "error.type", observability.ErrTypeValidation)

	for _, attr := range recorded.Attributes {
		assert.NotEqual(t, "error.source", string(attr.Key), "error.source should not be set when empty")
	}
}

func TestHTTPMiddleware_AccessLog(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("test")

	var buf bytes.Buffer

	logger := slog.New(slog.NewTextHandler(&buf, nil))

	handler := http.HandlerFunc(func(rw http.ResponseWriter, _ *http.Request) {
		rw.WriteHeader(http.StatusOK)
	})

	mw := observability.HTTPMiddleware(tracer, logger, handler)

	req := httptest.NewRequest(http.MethodGet, "/v1/analyze", http.NoBody)
	rec := httptest.NewRecorder()

	mw.ServeHTTP(rec, req)

	logOutput := buf.String()
	assert.Contains(t, logOutput, "http.request")
	assert.Contains(t, logOutput, "method=GET")
	assert.Contains(t, logOutput, "path=/v1/analyze")
	assert.Contains(t, logOutput, "status=200")
	assert.Contains(t, logOutput, "duration_ms=")
}

func assertAttribute(t *testing.T, attrs []attribute.KeyValue, key, wantValue string) {
	t.Helper()

	for _, attr := range attrs {
		if string(attr.Key) == key {
			assert.Equal(t, wantValue, attr.Value.AsString())

			return
		}
	}

	t.Errorf("attribute %q not found", key)
}

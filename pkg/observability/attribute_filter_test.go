package observability_test

import (
	"bytes"
	"context"
	"log/slog"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.opentelemetry.io/otel/attribute"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

func TestAttributeFilter_AllowsKnownKeys(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	filter := observability.NewAttributeFilter(sdktrace.NewSimpleSpanProcessor(exporter), nil)
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSpanProcessor(filter),
		sdktrace.WithSampler(sdktrace.AlwaysSample()),
	)

	_, span := tp.Tracer("test").Start(context.Background(), "op")
	span.SetAttributes(
		attribute.String("error.type", "timeout"),
		attribute.Int("chunk.size", 100),
	)
	span.End()

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)
	attrs := spanAttrMap(spans[0])
	assert.Equal(t, "timeout", attrs["error.type"])
	assert.Equal(t, int64(100), attrs["chunk.size"])
}

func TestAttributeFilter_BlocksPII(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	filter := observability.NewAttributeFilter(sdktrace.NewSimpleSpanProcessor(exporter), nil)
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSpanProcessor(filter),
		sdktrace.WithSampler(sdktrace.AlwaysSample()),
	)

	_, span := tp.Tracer("test").Start(context.Background(), "op")
	span.SetAttributes(
		attribute.String("user.email", "alice@example.com"),
		attribute.String("email", "bob@example.com"),
		attribute.String("request.body", "{\"password\":\"secret\"}"),
		attribute.String("response.body", "{\"token\":\"abc\"}"),
		attribute.String("user.id", "12345"),
		attribute.String("error.type", "internal"),
	)
	span.End()

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)
	attrs := spanAttrMap(spans[0])

	// PII keys must be stripped.
	assert.NotContains(t, attrs, "user.email")
	assert.NotContains(t, attrs, "email")
	assert.NotContains(t, attrs, "request.body")
	assert.NotContains(t, attrs, "response.body")
	assert.NotContains(t, attrs, "user.id")

	// Allowed key must be preserved.
	assert.Equal(t, "internal", attrs["error.type"])
}

func TestAttributeFilter_WarnsInDevMode(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()

	var buf bytes.Buffer

	logger := slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{Level: slog.LevelWarn}))

	filter := observability.NewAttributeFilter(sdktrace.NewSimpleSpanProcessor(exporter), logger)
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSpanProcessor(filter),
		sdktrace.WithSampler(sdktrace.AlwaysSample()),
	)

	_, span := tp.Tracer("test").Start(context.Background(), "op")
	span.SetAttributes(
		attribute.String("user.secret", "val"),
	)
	span.End()

	assert.Contains(t, buf.String(), "user.secret")
	assert.Contains(t, buf.String(), "blocked")
}

func TestAttributeFilter_PassesUnknownAllowedPrefixes(t *testing.T) {
	t.Parallel()

	exporter := tracetest.NewInMemoryExporter()
	filter := observability.NewAttributeFilter(sdktrace.NewSimpleSpanProcessor(exporter), nil)
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSpanProcessor(filter),
		sdktrace.WithSampler(sdktrace.AlwaysSample()),
	)

	_, span := tp.Tracer("test").Start(context.Background(), "op")
	span.SetAttributes(
		attribute.String("codefang.new_attr", "val"),
		attribute.String("http.method", "GET"),
		attribute.String("error.source", "client"),
	)
	span.End()

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)
	attrs := spanAttrMap(spans[0])
	assert.Equal(t, "val", attrs["codefang.new_attr"])
	assert.Equal(t, "GET", attrs["http.method"])
	assert.Equal(t, "client", attrs["error.source"])
}

// spanAttrMap converts a span's attributes into a map for easy assertion.
func spanAttrMap(s tracetest.SpanStub) map[string]any {
	m := make(map[string]any, len(s.Attributes))
	for _, a := range s.Attributes {
		m[string(a.Key)] = a.Value.AsInterface()
	}

	return m
}

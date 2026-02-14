package observability_test

import (
	"bytes"
	"context"
	"encoding/json"
	"log/slog"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

func TestTracingHandler_InjectsTraceContext(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	inner := slog.NewJSONHandler(&buf, &slog.HandlerOptions{Level: slog.LevelDebug})
	handler := observability.NewTracingHandler(inner, "test-svc", "test", observability.ModeCLI)
	logger := slog.New(handler)

	// Create a span context with known trace and span IDs.
	traceID, err := trace.TraceIDFromHex("0102030405060708090a0b0c0d0e0f10")
	require.NoError(t, err)

	spanID, err := trace.SpanIDFromHex("0102030405060708")
	require.NoError(t, err)

	sc := trace.NewSpanContext(trace.SpanContextConfig{
		TraceID:    traceID,
		SpanID:     spanID,
		TraceFlags: trace.FlagsSampled,
	})
	ctx := trace.ContextWithSpanContext(context.Background(), sc)

	logger.InfoContext(ctx, "test message")

	var record map[string]any

	err = json.Unmarshal(buf.Bytes(), &record)
	require.NoError(t, err)

	assert.Equal(t, "0102030405060708090a0b0c0d0e0f10", record["trace_id"])
	assert.Equal(t, "0102030405060708", record["span_id"])
	assert.Equal(t, "test-svc", record["service"])
	assert.Equal(t, "test", record["env"])
	assert.Equal(t, "cli", record["mode"])
}

func TestTracingHandler_NoTraceContext(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	inner := slog.NewJSONHandler(&buf, &slog.HandlerOptions{Level: slog.LevelDebug})
	handler := observability.NewTracingHandler(inner, "codefang", "", observability.ModeMCP)
	logger := slog.New(handler)

	logger.InfoContext(context.Background(), "no span")

	var record map[string]any

	err := json.Unmarshal(buf.Bytes(), &record)
	require.NoError(t, err)

	// No trace_id or span_id should be present without active span.
	_, hasTraceID := record["trace_id"]
	assert.False(t, hasTraceID)

	// Service and mode should still be present.
	assert.Equal(t, "codefang", record["service"])
	assert.Equal(t, "mcp", record["mode"])
}

func TestTracingHandler_WithGroup(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	inner := slog.NewJSONHandler(&buf, &slog.HandlerOptions{Level: slog.LevelDebug})
	handler := observability.NewTracingHandler(inner, "codefang", "", observability.ModeCLI)
	logger := slog.New(handler)

	grouped := logger.WithGroup("pipeline")
	grouped.InfoContext(context.Background(), "stage done", slog.String("stage", "blob"))

	var record map[string]any

	err := json.Unmarshal(buf.Bytes(), &record)
	require.NoError(t, err)

	// Service attrs should be at top level.
	assert.Equal(t, "codefang", record["service"])

	// Grouped attrs should be nested.
	pipeline, ok := record["pipeline"].(map[string]any)
	require.True(t, ok)
	assert.Equal(t, "blob", pipeline["stage"])
}

func TestTracingHandler_WithAttrs(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	inner := slog.NewJSONHandler(&buf, &slog.HandlerOptions{Level: slog.LevelDebug})
	handler := observability.NewTracingHandler(inner, "codefang", "", observability.ModeCLI)
	logger := slog.New(handler)

	withAttrs := logger.With(slog.String("op", "analyze"))
	withAttrs.InfoContext(context.Background(), "started")

	var record map[string]any

	err := json.Unmarshal(buf.Bytes(), &record)
	require.NoError(t, err)

	assert.Equal(t, "analyze", record["op"])
	assert.Equal(t, "codefang", record["service"])
}

package observability_test

import (
	"bytes"
	"context"
	"encoding/json"
	"log/slog"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	sdkmetric "go.opentelemetry.io/otel/sdk/metric"
	"go.opentelemetry.io/otel/sdk/metric/metricdata"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

// acceptanceSpanCount is the expected number of spans in the acceptance test
// (root + chunk + analyze).
const acceptanceSpanCount = 3

// acceptanceCommitCount is the simulated commit count used in log assertions.
const acceptanceCommitCount = 42

// TestAcceptance_EndToEnd verifies all three observability signals (traces,
// metrics, structured logs with trace context) work together in a single
// simulated pipeline run.
func TestAcceptance_EndToEnd(t *testing.T) {
	t.Parallel()

	// Setup: in-memory trace exporter.
	spanExporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(spanExporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	tracer := tp.Tracer("codefang")

	// Setup: in-memory metric reader.
	metricReader := sdkmetric.NewManualReader()
	mp := sdkmetric.NewMeterProvider(sdkmetric.WithReader(metricReader))
	meter := mp.Meter("codefang")

	red, err := observability.NewREDMetrics(meter)
	require.NoError(t, err)

	analysis, err := observability.NewAnalysisMetrics(meter)
	require.NoError(t, err)

	// Setup: structured logger with trace context.
	var logBuf bytes.Buffer

	innerHandler := slog.NewJSONHandler(&logBuf, &slog.HandlerOptions{Level: slog.LevelDebug})
	tracingHandler := observability.NewTracingHandler(innerHandler, "codefang", "test", observability.ModeCLI)
	logger := slog.New(tracingHandler)

	// Simulate pipeline: root span, child spans, metrics, logs.
	ctx, rootSpan := tracer.Start(context.Background(), "codefang.run")

	_, chunkSpan := tracer.Start(ctx, "codefang.chunk")
	chunkSpan.End()

	_, analyzeSpan := tracer.Start(ctx, "codefang.analyzer.Burndown")
	analyzeSpan.End()

	// Record metrics within the trace context.
	red.RecordRequest(ctx, "cli.run", "ok", time.Second)

	analysis.RecordRun(ctx, observability.AnalysisStats{
		Commits:         acceptanceCommitCount,
		Chunks:          3,
		ChunkDurations:  []time.Duration{time.Second, 2 * time.Second, 3 * time.Second},
		BlobCacheHits:   100,
		BlobCacheMisses: 10,
		DiffCacheHits:   50,
		DiffCacheMisses: 5,
	})

	// Emit a log line within the trace context.
	logger.InfoContext(ctx, "pipeline.complete", "commits", acceptanceCommitCount)

	rootSpan.End()

	// Assert: Traces.
	spans := spanExporter.GetSpans()
	require.Len(t, spans, acceptanceSpanCount, "expected root + 2 child spans")

	spanNames := make(map[string]bool, len(spans))
	for _, s := range spans {
		spanNames[s.Name] = true
	}

	assert.True(t, spanNames["codefang.run"], "root span should exist")
	assert.True(t, spanNames["codefang.chunk"], "chunk span should exist")
	assert.True(t, spanNames["codefang.analyzer.Burndown"], "analyzer span should exist")

	// All spans share the same trace ID.
	traceID := spans[0].SpanContext.TraceID()
	for _, s := range spans[1:] {
		assert.Equal(t, traceID, s.SpanContext.TraceID(),
			"span %q should share trace ID", s.Name)
	}

	// Assert: Metrics.
	var rm metricdata.ResourceMetrics

	err = metricReader.Collect(ctx, &rm)
	require.NoError(t, err)

	reqTotal := findMetric(rm, "codefang.requests.total")
	require.NotNil(t, reqTotal, "request counter should be recorded")

	reqDuration := findMetric(rm, "codefang.request.duration.seconds")
	require.NotNil(t, reqDuration, "duration histogram should be recorded")

	// Assert: Analysis metrics.
	commitsTotal := findMetric(rm, "codefang.analysis.commits.total")
	require.NotNil(t, commitsTotal, "analysis commits counter should be recorded")

	chunksTotal := findMetric(rm, "codefang.analysis.chunks.total")
	require.NotNil(t, chunksTotal, "analysis chunks counter should be recorded")

	chunkDuration := findMetric(rm, "codefang.analysis.chunk.duration.seconds")
	require.NotNil(t, chunkDuration, "chunk duration histogram should be recorded")

	cacheHits := findMetric(rm, "codefang.analysis.cache.hits.total")
	require.NotNil(t, cacheHits, "cache hits counter should be recorded")

	cacheMisses := findMetric(rm, "codefang.analysis.cache.misses.total")
	require.NotNil(t, cacheMisses, "cache misses counter should be recorded")

	// Assert: Logs contain trace_id.
	var logRecord map[string]any

	err = json.Unmarshal(logBuf.Bytes(), &logRecord)
	require.NoError(t, err)

	assert.Equal(t, traceID.String(), logRecord["trace_id"],
		"log line should contain the active trace_id")
	assert.Contains(t, logRecord, "span_id",
		"log line should contain span_id")
	assert.Equal(t, "codefang", logRecord["service"],
		"log line should contain service name")

	commits, ok := logRecord["commits"].(float64)
	require.True(t, ok, "commits should be a number")
	assert.InDelta(t, acceptanceCommitCount, commits, 0,
		"log line should contain custom attributes")
}

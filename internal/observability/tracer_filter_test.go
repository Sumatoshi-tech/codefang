package observability_test

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"
	"go.opentelemetry.io/otel/trace"
	nooptrace "go.opentelemetry.io/otel/trace/noop"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
)

func newTestProvider() (*tracetest.InMemoryExporter, trace.TracerProvider) {
	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSyncer(exporter),
		sdktrace.WithSampler(sdktrace.AlwaysSample()),
	)

	return exporter, tp
}

func TestFilteringProvider_SuppressedTracer(t *testing.T) {
	t.Parallel()

	exporter, base := newTestProvider()
	fp := observability.NewFilteringTracerProvider(base)

	// codefang.gitlib is suppressed — spans should not be recorded.
	tracer := fp.Tracer("codefang.gitlib")
	_, span := tracer.Start(context.Background(), "git.lookup_commit")
	span.End()

	assert.Empty(t, exporter.GetSpans(), "suppressed tracer should produce no exported spans")
}

func TestFilteringProvider_SuppressedSpan(t *testing.T) {
	t.Parallel()

	exporter, base := newTestProvider()
	fp := observability.NewFilteringTracerProvider(base)

	tracer := fp.Tracer("codefang.framework")

	// Structural span should pass through.
	_, structSpan := tracer.Start(context.Background(), "codefang.runner.run")
	structSpan.End()

	// Hot-path span should be suppressed.
	_, hotSpan := tracer.Start(context.Background(), "codefang.analyzer.consume")
	hotSpan.End()

	spans := exporter.GetSpans()
	require.Len(t, spans, 1, "only structural span should be exported")
	assert.Equal(t, "codefang.runner.run", spans[0].Name)
}

func TestFilteringProvider_PassThrough(t *testing.T) {
	t.Parallel()

	exporter, base := newTestProvider()
	fp := observability.NewFilteringTracerProvider(base)

	// Root "codefang" tracer is not suppressed — spans pass through,
	// but span-level filtering still applies (codefang.analyzer.consume).
	tracer := fp.Tracer("codefang")
	_, span := tracer.Start(context.Background(), "codefang.some_operation")
	span.End()

	spans := exporter.GetSpans()
	require.Len(t, spans, 1)
	assert.Equal(t, "codefang.some_operation", spans[0].Name)
}

func TestFilteringProvider_UASTParseSuppressed(t *testing.T) {
	t.Parallel()

	exporter, base := newTestProvider()
	fp := observability.NewFilteringTracerProvider(base)

	tracer := fp.Tracer("codefang.uast")
	_, span := tracer.Start(context.Background(), "uast.parse")
	span.End()

	assert.Empty(t, exporter.GetSpans(), "UAST parse spans should be suppressed")
}

func TestFilteringProvider_NoopSpanIsValid(t *testing.T) {
	t.Parallel()

	fp := observability.NewFilteringTracerProvider(nooptrace.NewTracerProvider())

	tracer := fp.Tracer("codefang.gitlib")
	ctx, span := tracer.Start(context.Background(), "git.lookup_blob")

	// Noop span should still be usable without panicking.
	span.SetName("renamed")
	span.End()

	assert.NotNil(t, ctx)
}

package framework_test

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// newTestProvider creates an in-memory tracer provider and exporter for tests.
func newTestProvider(t *testing.T) (*tracetest.InMemoryExporter, trace.Tracer) {
	t.Helper()

	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(sdktrace.WithSyncer(exporter))

	t.Cleanup(func() { require.NoError(t, tp.Shutdown(context.Background())) })

	return exporter, tp.Tracer("codefang")
}

// TestRunner_SpanParentChild verifies that Runner.Run creates a codefang.chunk
// span that is a child of the caller's span.
func TestRunner_SpanParentChild(t *testing.T) {
	t.Parallel()

	exporter, tracer := newTestProvider(t)

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("a.txt", "hello")
	repo.Commit("first")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	require.NoError(t, err)

	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 1)
	require.Len(t, commits, 1)

	config := framework.DefaultCoordinatorConfig()
	config.UASTPipelineWorkers = 0 // Disable UAST pipeline for simpler span tree.

	runner := framework.NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})
	runner.Tracer = tracer

	// Create a root span to verify parent-child chain.
	ctx, rootSpan := tracer.Start(context.Background(), "codefang.run")

	reports, err := runner.Run(ctx, commits)
	require.NoError(t, err)
	require.Len(t, reports, 1)

	rootSpan.End()

	spans := exporter.GetSpans()

	// Find spans by name.
	spansByName := make(map[string][]tracetest.SpanStub)
	for _, s := range spans {
		spansByName[s.Name] = append(spansByName[s.Name], s)
	}

	// Verify codefang.run exists.
	require.Contains(t, spansByName, "codefang.run", "root span should exist")
	rootStub := spansByName["codefang.run"][0]

	// Verify codefang.chunk exists and is child of root.
	require.Contains(t, spansByName, "codefang.chunk", "chunk span should exist")
	chunkStub := spansByName["codefang.chunk"][0]

	assert.Equal(t, rootStub.SpanContext.TraceID(), chunkStub.SpanContext.TraceID(),
		"chunk span should share root trace ID")
	assert.Equal(t, rootStub.SpanContext.SpanID(), chunkStub.Parent.SpanID(),
		"chunk span should be child of root span")

	// Verify chunk attributes.
	chunkAttrs := attrMap(chunkStub)
	assert.Equal(t, int64(1), chunkAttrs["chunk.size"], "chunk.size should be 1")
	assert.Equal(t, int64(0), chunkAttrs["chunk.offset"], "chunk.offset should be 0")
	assert.Equal(t, int64(0), chunkAttrs["chunk.index"], "chunk.index should be 0")

	// Verify pipeline attributes on the chunk span.
	assert.Contains(t, chunkAttrs, "pipeline.blob_ms", "chunk should have pipeline.blob_ms")
	assert.Contains(t, chunkAttrs, "pipeline.diff_ms", "chunk should have pipeline.diff_ms")
	assert.Contains(t, chunkAttrs, "cache.blob.hits", "chunk should have cache.blob.hits")
	assert.Contains(t, chunkAttrs, "cache.blob.misses", "chunk should have cache.blob.misses")

	// Pipeline spans should NOT exist (removed in favor of attributes).
	assert.NotContains(t, spansByName, "codefang.pipeline", "pipeline span should not exist")
	assert.NotContains(t, spansByName, "codefang.pipeline.blob", "blob stage span should not exist")
	assert.NotContains(t, spansByName, "codefang.pipeline.diff", "diff stage span should not exist")
}

// TestRunner_SpanUnbrokenChain verifies that ALL spans in the trace share the same
// trace ID, ensuring an unbroken parent-child chain.
func TestRunner_SpanUnbrokenChain(t *testing.T) {
	t.Parallel()

	exporter, tracer := newTestProvider(t)

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	repo.CreateFile("b.txt", "world")
	repo.Commit("init")
	repo.CreateFile("b.txt", "changed")
	repo.Commit("update")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	require.NoError(t, err)

	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 0)
	require.GreaterOrEqual(t, len(commits), 2)

	config := framework.DefaultCoordinatorConfig()
	config.UASTPipelineWorkers = 0

	runner := framework.NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})
	runner.Tracer = tracer

	ctx, rootSpan := tracer.Start(context.Background(), "codefang.run")

	_, err = runner.Run(ctx, commits)
	require.NoError(t, err)

	rootSpan.End()

	spans := exporter.GetSpans()
	require.NotEmpty(t, spans)

	// All spans must share the same trace ID.
	traceID := spans[0].SpanContext.TraceID()
	for _, s := range spans {
		assert.Equal(t, traceID, s.SpanContext.TraceID(),
			"span %q should share trace ID", s.Name)
	}
}

// TestSpanAttributes_CacheStats verifies that the chunk span carries
// pipeline and cache attributes.
func TestSpanAttributes_CacheStats(t *testing.T) {
	t.Parallel()

	exporter, tracer := newTestProvider(t)

	repo := framework.NewTestRepo(t)
	defer repo.Close()

	// Two commits touching the same file to generate cache lookups.
	repo.CreateFile("a.txt", "version1")
	repo.Commit("first")
	repo.CreateFile("a.txt", "version2")
	repo.Commit("second")

	libRepo, err := gitlib.OpenRepository(repo.Path())
	require.NoError(t, err)

	defer libRepo.Free()

	commits := framework.CollectCommits(t, libRepo, 0)
	require.GreaterOrEqual(t, len(commits), 2)

	config := framework.DefaultCoordinatorConfig()
	config.UASTPipelineWorkers = 0
	config.BlobCacheSize = framework.DefaultGlobalCacheSize

	runner := framework.NewRunnerWithConfig(libRepo, repo.Path(), config, &plumbing.TreeDiffAnalyzer{})
	runner.Tracer = tracer

	ctx, rootSpan := tracer.Start(context.Background(), "test.root")

	_, err = runner.Run(ctx, commits)
	require.NoError(t, err)

	rootSpan.End()

	spans := exporter.GetSpans()

	// Find the chunk span (was previously on blob stage span).
	var chunkSpan *tracetest.SpanStub

	for i := range spans {
		if spans[i].Name == "codefang.chunk" {
			chunkSpan = &spans[i]

			break
		}
	}

	require.NotNil(t, chunkSpan, "chunk span should exist")

	attrs := attrMap(*chunkSpan)
	assert.Contains(t, attrs, "cache.blob.hits", "chunk span should have cache.blob.hits attribute")
	assert.Contains(t, attrs, "cache.blob.misses", "chunk span should have cache.blob.misses attribute")
	assert.Contains(t, attrs, "pipeline.blob_ms", "chunk span should have pipeline.blob_ms attribute")
	assert.Contains(t, attrs, "pipeline.diff_ms", "chunk span should have pipeline.diff_ms attribute")
}

// attrMap converts span attributes to a map for easy lookup.
func attrMap(stub tracetest.SpanStub) map[string]any {
	m := make(map[string]any, len(stub.Attributes))
	for _, attr := range stub.Attributes {
		m[string(attr.Key)] = attr.Value.AsInterface()
	}

	return m
}

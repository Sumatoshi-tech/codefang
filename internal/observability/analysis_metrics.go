package observability

import (
	"context"
	"fmt"
	"time"

	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/metric"
)

const (
	metricCommitsTotal     = "codefang.analysis.commits.total"
	metricChunksTotal      = "codefang.analysis.chunks.total"
	metricChunkDuration    = "codefang.analysis.chunk.duration.seconds"
	metricCacheHitsTotal   = "codefang.analysis.cache.hits.total"
	metricCacheMissesTotal = "codefang.analysis.cache.misses.total"

	attrCache = "cache"
)

// AnalysisMetrics holds OTel instruments for analysis-specific metrics.
type AnalysisMetrics struct {
	commitsTotal  metric.Int64Counter
	chunksTotal   metric.Int64Counter
	chunkDuration metric.Float64Histogram
	cacheHits     metric.Int64Counter
	cacheMisses   metric.Int64Counter
}

// AnalysisStats holds the statistics for a single streaming run,
// decoupled from framework types.
type AnalysisStats struct {
	Commits         int64
	Chunks          int
	ChunkDurations  []time.Duration
	BlobCacheHits   int64
	BlobCacheMisses int64
	DiffCacheHits   int64
	DiffCacheMisses int64
}

// NewAnalysisMetrics creates analysis metric instruments from the given meter.
func NewAnalysisMetrics(mt metric.Meter) (*AnalysisMetrics, error) {
	commits, err := mt.Int64Counter(metricCommitsTotal,
		metric.WithDescription("Total commits analyzed"),
		metric.WithUnit("{commit}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricCommitsTotal, err)
	}

	chunks, err := mt.Int64Counter(metricChunksTotal,
		metric.WithDescription("Total chunks processed"),
		metric.WithUnit("{chunk}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricChunksTotal, err)
	}

	chunkDur, err := mt.Float64Histogram(metricChunkDuration,
		metric.WithDescription("Per-chunk processing duration in seconds"),
		metric.WithUnit("s"),
		metric.WithExplicitBucketBoundaries(durationBucketBoundaries...),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricChunkDuration, err)
	}

	hits, err := mt.Int64Counter(metricCacheHitsTotal,
		metric.WithDescription("Cache hits by type"),
		metric.WithUnit("{hit}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricCacheHitsTotal, err)
	}

	misses, err := mt.Int64Counter(metricCacheMissesTotal,
		metric.WithDescription("Cache misses by type"),
		metric.WithUnit("{miss}"),
	)
	if err != nil {
		return nil, fmt.Errorf("create %s: %w", metricCacheMissesTotal, err)
	}

	return &AnalysisMetrics{
		commitsTotal:  commits,
		chunksTotal:   chunks,
		chunkDuration: chunkDur,
		cacheHits:     hits,
		cacheMisses:   misses,
	}, nil
}

// RecordRun records analysis statistics for a completed streaming run.
// Safe to call on a nil receiver (no-op).
func (am *AnalysisMetrics) RecordRun(ctx context.Context, stats AnalysisStats) {
	if am == nil {
		return
	}

	am.commitsTotal.Add(ctx, stats.Commits)
	am.chunksTotal.Add(ctx, int64(stats.Chunks))

	for _, d := range stats.ChunkDurations {
		am.chunkDuration.Record(ctx, d.Seconds())
	}

	blobAttrs := metric.WithAttributes(attribute.String(attrCache, "blob"))
	am.cacheHits.Add(ctx, stats.BlobCacheHits, blobAttrs)
	am.cacheMisses.Add(ctx, stats.BlobCacheMisses, blobAttrs)

	diffAttrs := metric.WithAttributes(attribute.String(attrCache, "diff"))
	am.cacheHits.Add(ctx, stats.DiffCacheHits, diffAttrs)
	am.cacheMisses.Add(ctx, stats.DiffCacheMisses, diffAttrs)
}

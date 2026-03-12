package observability

import (
	"context"
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
	return buildMetrics(mt, func(b *metricBuilder) *AnalysisMetrics {
		return &AnalysisMetrics{
			commitsTotal: createMetric(b, metricCommitsTotal, func() (metric.Int64Counter, error) {
				return b.meter.Int64Counter(metricCommitsTotal,
					metric.WithDescription("Total commits analyzed"), metric.WithUnit("{commit}"))
			}),
			chunksTotal: createMetric(b, metricChunksTotal, func() (metric.Int64Counter, error) {
				return b.meter.Int64Counter(metricChunksTotal,
					metric.WithDescription("Total chunks processed"), metric.WithUnit("{chunk}"))
			}),
			chunkDuration: createMetric(b, metricChunkDuration, func() (metric.Float64Histogram, error) {
				return b.meter.Float64Histogram(metricChunkDuration,
					metric.WithDescription("Per-chunk processing duration in seconds"),
					metric.WithUnit("s"),
					metric.WithExplicitBucketBoundaries(durationBucketBoundaries...))
			}),
			cacheHits: createMetric(b, metricCacheHitsTotal, func() (metric.Int64Counter, error) {
				return b.meter.Int64Counter(metricCacheHitsTotal,
					metric.WithDescription("Cache hits by type"), metric.WithUnit("{hit}"))
			}),
			cacheMisses: createMetric(b, metricCacheMissesTotal, func() (metric.Int64Counter, error) {
				return b.meter.Int64Counter(metricCacheMissesTotal,
					metric.WithDescription("Cache misses by type"), metric.WithUnit("{miss}"))
			}),
		}
	})
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

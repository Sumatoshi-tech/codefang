package observability

import (
	"context"
	"fmt"

	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/metric"
)

const (
	metricCacheHits   = "codefang.cache.hits"
	metricCacheMisses = "codefang.cache.misses"
)

// CacheStatsProvider exposes cache hit/miss counters for OTel export.
type CacheStatsProvider interface {
	CacheHits() int64
	CacheMisses() int64
}

// RegisterCacheMetrics registers observable gauges that report cache hit/miss
// counters from blob and diff caches. Either provider may be nil.
func RegisterCacheMetrics(mt metric.Meter, blob, diff CacheStatsProvider) error {
	providers := make([]struct {
		name     string
		provider CacheStatsProvider
	}, 0, 2) // Two cache types: blob and diff.

	if blob != nil {
		providers = append(providers, struct {
			name     string
			provider CacheStatsProvider
		}{"blob", blob})
	}

	if diff != nil {
		providers = append(providers, struct {
			name     string
			provider CacheStatsProvider
		}{"diff", diff})
	}

	if len(providers) == 0 {
		return nil
	}

	_, err := mt.Int64ObservableGauge(metricCacheHits,
		metric.WithDescription("Cache hit count"),
		metric.WithUnit("{hit}"),
		metric.WithInt64Callback(func(_ context.Context, o metric.Int64Observer) error {
			for _, p := range providers {
				o.Observe(p.provider.CacheHits(), metric.WithAttributes(
					attribute.String("cache", p.name),
				))
			}

			return nil
		}),
	)
	if err != nil {
		return fmt.Errorf("create %s: %w", metricCacheHits, err)
	}

	_, err = mt.Int64ObservableGauge(metricCacheMisses,
		metric.WithDescription("Cache miss count"),
		metric.WithUnit("{miss}"),
		metric.WithInt64Callback(func(_ context.Context, o metric.Int64Observer) error {
			for _, p := range providers {
				o.Observe(p.provider.CacheMisses(), metric.WithAttributes(
					attribute.String("cache", p.name),
				))
			}

			return nil
		}),
	)
	if err != nil {
		return fmt.Errorf("create %s: %w", metricCacheMisses, err)
	}

	return nil
}

package framework

import (
	"sync/atomic"
)

// StageMetrics provides per-stage high-watermark counters for memory triage.
// All fields are updated atomically by pipeline stages and read by the sampler.
// Following the playbook: "diff items queued", "bytes of blob content held",
// "AST cache entries", "results map size".
type StageMetrics struct {
	// Blob pipeline metrics.
	BlobChangesInFlight atomic.Int64 // Number of file changes being processed.
	BlobBytesLoaded     atomic.Int64 // Total blob bytes loaded in current batch.
	BlobCacheEntries    atomic.Int64 // Current global blob cache entry count.
	BlobCacheBytes      atomic.Int64 // Current global blob cache byte size.

	// Diff pipeline metrics.
	DiffItemsQueued  atomic.Int64 // Diff requests pending in batcher.
	DiffCacheEntries atomic.Int64 // Current diff cache entry count.

	// UAST pipeline metrics.
	UASTItemsQueued atomic.Int64 // UAST parse jobs pending.

	// Runner / aggregator metrics.
	AggregatorBytes  atomic.Int64 // Estimated aggregator state size.
	CommitsProcessed atomic.Int64 // Commits processed in current chunk.
	LastChangeCount  atomic.Int64 // File changes in most recent commit.

	// High-watermarks (updated by Record* methods, never decrease within a chunk).
	PeakBlobChanges atomic.Int64 // Max changes seen in any single commit.
	PeakBlobBytes   atomic.Int64 // Max blob bytes loaded in any single batch.
	PeakDiffQueued  atomic.Int64 // Max diff items queued at any point.
}

// RecordBlobBatch updates blob metrics and high-watermarks for a batch.
func (m *StageMetrics) RecordBlobBatch(changes, blobBytes int64) {
	m.BlobChangesInFlight.Store(changes)
	m.BlobBytesLoaded.Store(blobBytes)

	for {
		peak := m.PeakBlobChanges.Load()
		if changes <= peak || m.PeakBlobChanges.CompareAndSwap(peak, changes) {
			break
		}
	}

	for {
		peak := m.PeakBlobBytes.Load()
		if blobBytes <= peak || m.PeakBlobBytes.CompareAndSwap(peak, blobBytes) {
			break
		}
	}
}

// RecordDiffQueue updates diff queue depth and high-watermark.
func (m *StageMetrics) RecordDiffQueue(queued int64) {
	m.DiffItemsQueued.Store(queued)

	for {
		peak := m.PeakDiffQueued.Load()
		if queued <= peak || m.PeakDiffQueued.CompareAndSwap(peak, queued) {
			break
		}
	}
}

// RecordCommit updates per-commit metrics.
func (m *StageMetrics) RecordCommit(changeCount int64) {
	m.CommitsProcessed.Add(1)
	m.LastChangeCount.Store(changeCount)

	for {
		peak := m.PeakBlobChanges.Load()
		if changeCount <= peak || m.PeakBlobChanges.CompareAndSwap(peak, changeCount) {
			break
		}
	}
}

// Reset clears all counters and watermarks for a new chunk.
func (m *StageMetrics) Reset() {
	m.BlobChangesInFlight.Store(0)
	m.BlobBytesLoaded.Store(0)
	m.BlobCacheEntries.Store(0)
	m.BlobCacheBytes.Store(0)
	m.DiffItemsQueued.Store(0)
	m.DiffCacheEntries.Store(0)
	m.UASTItemsQueued.Store(0)
	m.AggregatorBytes.Store(0)
	m.CommitsProcessed.Store(0)
	m.LastChangeCount.Store(0)
	m.PeakBlobChanges.Store(0)
	m.PeakBlobBytes.Store(0)
	m.PeakDiffQueued.Store(0)
}

// Snapshot returns a point-in-time copy of all metrics.
type StageMetricsSnapshot struct {
	BlobChangesInFlight int64
	BlobBytesLoaded     int64
	BlobCacheEntries    int64
	BlobCacheBytes      int64
	DiffItemsQueued     int64
	DiffCacheEntries    int64
	UASTItemsQueued     int64
	AggregatorBytes     int64
	CommitsProcessed    int64
	LastChangeCount     int64
	PeakBlobChanges     int64
	PeakBlobBytes       int64
	PeakDiffQueued      int64
}

// Snapshot reads all counters atomically (each field individually).
func (m *StageMetrics) Snapshot() StageMetricsSnapshot {
	return StageMetricsSnapshot{
		BlobChangesInFlight: m.BlobChangesInFlight.Load(),
		BlobBytesLoaded:     m.BlobBytesLoaded.Load(),
		BlobCacheEntries:    m.BlobCacheEntries.Load(),
		BlobCacheBytes:      m.BlobCacheBytes.Load(),
		DiffItemsQueued:     m.DiffItemsQueued.Load(),
		DiffCacheEntries:    m.DiffCacheEntries.Load(),
		UASTItemsQueued:     m.UASTItemsQueued.Load(),
		AggregatorBytes:     m.AggregatorBytes.Load(),
		CommitsProcessed:    m.CommitsProcessed.Load(),
		LastChangeCount:     m.LastChangeCount.Load(),
		PeakBlobChanges:     m.PeakBlobChanges.Load(),
		PeakBlobBytes:       m.PeakBlobBytes.Load(),
		PeakDiffQueued:      m.PeakDiffQueued.Load(),
	}
}

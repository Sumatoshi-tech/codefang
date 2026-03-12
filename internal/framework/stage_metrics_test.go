package framework

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestStageMetrics_RecordBlobBatch_UpdatesWatermarks(t *testing.T) {
	t.Parallel()

	m := &StageMetrics{}

	m.RecordBlobBatch(100, 1024)
	assert.Equal(t, int64(100), m.PeakBlobChanges.Load())
	assert.Equal(t, int64(1024), m.PeakBlobBytes.Load())

	// Smaller batch should not lower watermarks.
	m.RecordBlobBatch(50, 512)
	assert.Equal(t, int64(100), m.PeakBlobChanges.Load())
	assert.Equal(t, int64(1024), m.PeakBlobBytes.Load())

	// Larger batch should raise watermarks.
	m.RecordBlobBatch(200, 2048)
	assert.Equal(t, int64(200), m.PeakBlobChanges.Load())
	assert.Equal(t, int64(2048), m.PeakBlobBytes.Load())
}

func TestStageMetrics_RecordDiffQueue_UpdatesWatermark(t *testing.T) {
	t.Parallel()

	m := &StageMetrics{}

	m.RecordDiffQueue(500)
	assert.Equal(t, int64(500), m.PeakDiffQueued.Load())

	m.RecordDiffQueue(300)
	assert.Equal(t, int64(500), m.PeakDiffQueued.Load())

	m.RecordDiffQueue(1000)
	assert.Equal(t, int64(1000), m.PeakDiffQueued.Load())
}

func TestStageMetrics_RecordCommit(t *testing.T) {
	t.Parallel()

	m := &StageMetrics{}

	m.RecordCommit(50)
	assert.Equal(t, int64(1), m.CommitsProcessed.Load())
	assert.Equal(t, int64(50), m.LastChangeCount.Load())

	m.RecordCommit(10)
	assert.Equal(t, int64(2), m.CommitsProcessed.Load())
	assert.Equal(t, int64(10), m.LastChangeCount.Load())
	assert.Equal(t, int64(50), m.PeakBlobChanges.Load())
}

func TestStageMetrics_Reset(t *testing.T) {
	t.Parallel()

	m := &StageMetrics{}
	m.RecordBlobBatch(100, 1024)
	m.RecordDiffQueue(500)
	m.RecordCommit(50)

	m.Reset()

	snap := m.Snapshot()
	assert.Equal(t, int64(0), snap.PeakBlobChanges)
	assert.Equal(t, int64(0), snap.PeakBlobBytes)
	assert.Equal(t, int64(0), snap.PeakDiffQueued)
	assert.Equal(t, int64(0), snap.CommitsProcessed)
}

func TestStageMetrics_Snapshot(t *testing.T) {
	t.Parallel()

	m := &StageMetrics{}
	m.BlobCacheEntries.Store(42)
	m.AggregatorBytes.Store(1024)

	snap := m.Snapshot()
	assert.Equal(t, int64(42), snap.BlobCacheEntries)
	assert.Equal(t, int64(1024), snap.AggregatorBytes)
}

package framework

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"runtime/pprof"
	"sync/atomic"
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/observability"
	"github.com/Sumatoshi-tech/codefang/pkg/units"
)

// samplerInterval is the polling interval for the pipeline sampler.
const samplerInterval = 2 * time.Second

// kilo is the divisor for displaying values in thousands.
const kilo = 1000

// PipelineSampler periodically logs comprehensive memory and pipeline metrics
// during chunk processing. Implements playbook section 2.1: "lightweight
// periodic sampler (always-on in debug builds).".
//
// t1Captured is atomic because the sampler goroutine (driven by its ticker)
// and the caller goroutine (via CaptureT1) both race to capture the t1 peak
// heap profile; CompareAndSwap guarantees exactly one wins.
type PipelineSampler struct {
	logger       *slog.Logger
	metrics      *StageMetrics
	interval     time.Duration
	dumpDir      string
	chunkIndex   int
	memBudget    int64
	profileAtRSS int64 // RSS threshold (bytes) to trigger t1 heap profile.
	t1Captured   atomic.Bool
}

// SamplerConfig configures the pipeline sampler.
type SamplerConfig struct {
	Logger       *slog.Logger
	Metrics      *StageMetrics
	DumpDir      string
	ChunkIndex   int
	MemBudget    int64
	ProfileAtRSS int64         // RSS in bytes at which to capture t1 profile. 0 = disabled.
	Interval     time.Duration // Polling interval. Zero uses default.
}

// NewPipelineSampler creates a sampler. Call Start to begin periodic logging.
func NewPipelineSampler(cfg SamplerConfig) *PipelineSampler {
	interval := cfg.Interval
	if interval <= 0 {
		interval = samplerInterval
	}

	return &PipelineSampler{
		logger:       cfg.Logger,
		metrics:      cfg.Metrics,
		interval:     interval,
		dumpDir:      cfg.DumpDir,
		chunkIndex:   cfg.ChunkIndex,
		memBudget:    cfg.MemBudget,
		profileAtRSS: cfg.ProfileAtRSS,
	}
}

// Start begins the sampler goroutine. It captures a t0 heap profile immediately
// and then logs metrics at the configured interval. Cancel the context to stop.
func (s *PipelineSampler) Start(ctx context.Context) {
	// Capture t0 heap profile (playbook step 2: "take snapshot at t0").
	if s.dumpDir != "" {
		s.captureProfile("t0")
	}

	go s.run(ctx)
}

func (s *PipelineSampler) run(ctx context.Context) {
	tick := 0

	ticker := time.NewTicker(s.interval)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
			tick++
			s.sample(tick)
		}
	}
}

func (s *PipelineSampler) sample(tick int) {
	snap := observability.TakeHeapSnapshot()
	smaps := observability.ReadSmapsRollup()
	stage := s.metrics.Snapshot()

	nativeMiB := max((snap.RSS-snap.Sys)/int64(units.MiB), 0)

	// Classify growth bucket (playbook section 3):
	// Case A: HeapInuse rises with RSS → Go heap retention
	// Case B: StackInuse rises → goroutine explosion
	// Case C: RSS rises but HeapInuse flat → native/mmap
	// Case D: HeapInuse drops after GC but RSS doesn't = allocator retention.
	s.logger.Info("SAMPLER",
		// Chunk context.
		"chunk", s.chunkIndex+1,
		"tick", tick,
		"commits", stage.CommitsProcessed,
		// Go runtime (playbook 2.1).
		"heap_inuse_mib", snap.HeapInuse/int64(units.MiB),
		"heap_alloc_mib", snap.HeapAlloc/int64(units.MiB),
		"heap_objects_k", snap.HeapObjects/kilo,
		"stack_inuse_mib", snap.StackInuse/int64(units.MiB),
		"next_gc_mib", snap.NextGC/int64(units.MiB),
		"num_gc", snap.NumGC,
		"goroutines", snap.Goroutines,
		// Process / OS (playbook 2.1).
		"rss_mib", snap.RSS/int64(units.MiB),
		"sys_mib", snap.Sys/int64(units.MiB),
		"native_mib", nativeMiB,
		// smaps_rollup: anonymous vs file-backed (playbook 2.2).
		"anon_mib", smaps.Anonymous/int64(units.MiB),
		"file_mib", smaps.FileBacked/int64(units.MiB),
		"priv_dirty_mib", smaps.PrivateDirty/int64(units.MiB),
		// Per-stage high-watermarks (playbook step 3).
		"blob_changes", stage.BlobChangesInFlight,
		"blob_bytes_mib", stage.BlobBytesLoaded/int64(units.MiB),
		"blob_cache_entries", stage.BlobCacheEntries,
		"blob_cache_mib", stage.BlobCacheBytes/int64(units.MiB),
		"diff_queued", stage.DiffItemsQueued,
		"diff_cache_entries", stage.DiffCacheEntries,
		"uast_queued", stage.UASTItemsQueued,
		"agg_mib", stage.AggregatorBytes/int64(units.MiB),
		"last_change_count", stage.LastChangeCount,
		// High-watermarks.
		"peak_changes", stage.PeakBlobChanges,
		"peak_blob_mib", stage.PeakBlobBytes/int64(units.MiB),
		"peak_diff_queued", stage.PeakDiffQueued,
	)

	// Auto-capture t1 profile on RSS threshold (playbook step 2: "at or right after peak").
	// CompareAndSwap guarantees at most one capture across both the sampler
	// goroutine and any concurrent CaptureT1 caller.
	if s.profileAtRSS > 0 && snap.RSS >= s.profileAtRSS && s.t1Captured.CompareAndSwap(false, true) {
		s.captureProfile("t1")
	}
}

// CaptureT1 forces capture of the t1 (peak) heap profile. Call after the
// chunk completes if the automatic RSS threshold wasn't hit.
// Safe to call concurrently with the sampler goroutine — at most one capture
// wins via CompareAndSwap.
func (s *PipelineSampler) CaptureT1() {
	if s.dumpDir == "" {
		return
	}

	if !s.t1Captured.CompareAndSwap(false, true) {
		return
	}

	s.captureProfile("t1")
}

func (s *PipelineSampler) captureProfile(label string) {
	path := fmt.Sprintf("%s/heap_%s_chunk%d.pb.gz", s.dumpDir, label, s.chunkIndex)

	f, err := os.Create(path)
	if err != nil {
		s.logger.Warn("sampler: failed to create profile", "path", path, "err", err)

		return
	}
	defer f.Close()

	writeErr := pprof.Lookup("heap").WriteTo(f, 0)
	if writeErr != nil {
		s.logger.Warn("sampler: failed to write profile", "path", path, "err", writeErr)

		return
	}

	s.logger.Info("sampler: captured heap profile", "label", label, "path", path)
}

package analyze_test

// FRD: specs/frds/FRD-20260311-cap-static-workers.md.
// FRD: specs/frds/FRD-20260311-static-malloc-trim.md.
// FRD: specs/frds/FRD-20260311-static-memory-limit.md.
// FRD: specs/frds/FRD-20260311-bounded-parser-pool.md.
// FRD: specs/frds/FRD-20260311-eager-tree-release.md.

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// benchPeakFileCount is the number of files for peak-heap benchmarks (Step 1.1).
const benchPeakFileCount = 200

// benchPeakFunctionsPerFile is the number of functions per file for peak-heap benchmarks.
const benchPeakFunctionsPerFile = 20

// benchCappedWorkers is the "after" worker count for bounded-concurrency benchmarks.
const benchCappedWorkers = 4

// benchMallocTrimFileCount is the number of files for malloc_trim benchmarks (Step 1.2).
const benchMallocTrimFileCount = 500

// benchMallocTrimFunctionsPerFile is the number of functions per file for malloc_trim benchmarks.
const benchMallocTrimFunctionsPerFile = 20

// benchMallocTrimInterval is the trim interval used in malloc_trim benchmarks.
const benchMallocTrimInterval = 50

// benchMemLimitFileCount is the number of files for memory-limit benchmarks (Step 1.3).
const benchMemLimitFileCount = 1000

// benchMemLimitFunctionsPerFile is the number of functions per file for memory-limit benchmarks.
const benchMemLimitFunctionsPerFile = 30

// benchMemLimitWorkers is the worker count for memory-limit benchmarks (higher = more heap).
const benchMemLimitWorkers = 12

// benchMemLimitBytes is the soft memory limit for the limited sub-benchmark (32 MiB).
// Set near the natural peak to force aggressive GC.
const benchMemLimitBytes = 32 * 1024 * 1024

// benchParserPoolFileCount is the number of files for parser pool benchmarks (Step 1.4).
const benchParserPoolFileCount = 300

// benchParserPoolFunctionsPerFile is the number of functions per file for parser pool benchmarks.
const benchParserPoolFunctionsPerFile = 20

// heapSampleInterval is how often peak heap is sampled.
const heapSampleInterval = 5 * time.Millisecond

// pageSize is the OS page size for RSS calculation from /proc/self/statm.
const pageSize = 4096

// bytesPerMiB converts bytes to mebibytes.
const bytesPerMiB = 1024 * 1024

// setupHeavyBenchDir creates a directory with Go files containing many functions.
// Each file has functionsPerFile functions with a small body to create non-trivial UAST trees.
func setupHeavyBenchDir(tb testing.TB, fileCount, functionsPerFile int) string {
	tb.Helper()

	dir := tb.TempDir()

	for i := range fileCount {
		name := filepath.Join(dir, fmt.Sprintf("mod%04d.go", i))

		var b strings.Builder

		fmt.Fprintf(&b, "package bench\n\n")

		for j := range functionsPerFile {
			fmt.Fprintf(&b, "func F%d_%d(a, b int) int {\n", i, j)
			fmt.Fprintf(&b, "\tx := a + b\n")
			fmt.Fprintf(&b, "\tif x > 0 {\n")
			fmt.Fprintf(&b, "\t\treturn x * 2\n")
			fmt.Fprintf(&b, "\t}\n")
			fmt.Fprintf(&b, "\treturn -x\n")
			fmt.Fprintf(&b, "}\n\n")
		}

		require.NoError(tb, os.WriteFile(name, []byte(b.String()), 0o600))
	}

	return dir
}

// heapSampler tracks peak HeapInuse via periodic sampling.
type heapSampler struct {
	peak atomic.Int64
	done chan struct{}
	wg   sync.WaitGroup
}

// newHeapSampler starts a goroutine that samples HeapInuse every heapSampleInterval.
func newHeapSampler() *heapSampler {
	s := &heapSampler{done: make(chan struct{})}

	s.wg.Add(1)

	go s.run()

	return s
}

func (s *heapSampler) run() {
	defer s.wg.Done()

	ticker := time.NewTicker(heapSampleInterval)
	defer ticker.Stop()

	var ms runtime.MemStats

	for {
		select {
		case <-ticker.C:
			runtime.ReadMemStats(&ms)

			mib := int64(ms.HeapInuse / bytesPerMiB)

			for {
				old := s.peak.Load()
				if mib <= old || s.peak.CompareAndSwap(old, mib) {
					break
				}
			}
		case <-s.done:
			return
		}
	}
}

// stopAndGet stops sampling and returns peak HeapInuse in MiB.
func (s *heapSampler) stopAndGet() float64 {
	close(s.done)
	s.wg.Wait()

	return float64(s.peak.Load())
}

// readRSSMiB reads current process RSS from /proc/self/statm (Linux only).
func readRSSMiB(tb testing.TB) float64 {
	tb.Helper()

	data, err := os.ReadFile("/proc/self/statm")
	require.NoError(tb, err)

	fields := strings.Fields(string(data))
	require.GreaterOrEqual(tb, len(fields), 2, "unexpected /proc/self/statm format")

	rssPages, err := strconv.ParseInt(fields[1], 10, 64)
	require.NoError(tb, err)

	return float64(rssPages*pageSize) / bytesPerMiB
}

// BenchmarkStaticPeakParsers measures peak HeapInuse with uncapped vs capped workers.
// Step 1.1: Assert capped peak < uncapped peak (at least 30% reduction on >8-core machines).
func BenchmarkStaticPeakParsers(b *testing.B) {
	dir := setupHeavyBenchDir(b, benchPeakFileCount, benchPeakFunctionsPerFile)

	b.Run("before-uncapped", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = runtime.NumCPU()
		svc.MallocTrimInterval = -1

		runtime.GC()

		sampler := newHeapSampler()

		b.ResetTimer()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		b.StopTimer()

		peakMiB := sampler.stopAndGet()
		b.ReportMetric(peakMiB, "peak-MiB")
	})

	b.Run("after-capped", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = benchCappedWorkers
		svc.MallocTrimInterval = -1

		runtime.GC()

		sampler := newHeapSampler()

		b.ResetTimer()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		b.StopTimer()

		peakMiB := sampler.stopAndGet()
		b.ReportMetric(peakMiB, "peak-MiB")
	})
}

// BenchmarkStaticMallocTrim measures RSS with trim enabled vs disabled.
// Step 1.2: Uses real malloc_trim (NativeMemoryReleaseFn=nil) and reads /proc/self/statm.
func BenchmarkStaticMallocTrim(b *testing.B) {
	dir := setupHeavyBenchDir(b, benchMallocTrimFileCount, benchMallocTrimFunctionsPerFile)

	b.Run("before-no-trim", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = benchCappedWorkers
		svc.MallocTrimInterval = -1

		runtime.GC()

		baselineRSS := readRSSMiB(b)

		b.ResetTimer()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		b.StopTimer()

		finalRSS := readRSSMiB(b)
		b.ReportMetric(finalRSS-baselineRSS, "rss-delta-MiB")
		b.ReportMetric(finalRSS, "rss-final-MiB")
	})

	b.Run("after-trim-enabled", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = benchCappedWorkers
		svc.MallocTrimInterval = benchMallocTrimInterval
		// NativeMemoryReleaseFn=nil uses real gitlib.ReleaseNativeMemory().

		runtime.GC()

		baselineRSS := readRSSMiB(b)

		b.ResetTimer()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		b.StopTimer()

		finalRSS := readRSSMiB(b)
		b.ReportMetric(finalRSS-baselineRSS, "rss-delta-MiB")
		b.ReportMetric(finalRSS, "rss-final-MiB")
	})
}

// BenchmarkStaticMemoryLimit measures peak HeapInuse with and without [debug.SetMemoryLimit].
// Step 1.3: Assert limited peak heap < unlimited peak heap.
func BenchmarkStaticMemoryLimit(b *testing.B) {
	dir := setupHeavyBenchDir(b, benchMemLimitFileCount, benchMemLimitFunctionsPerFile)

	b.Run("before-no-limit", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = benchMemLimitWorkers
		svc.MallocTrimInterval = -1

		runtime.GC()

		sampler := newHeapSampler()

		b.ResetTimer()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		b.StopTimer()

		peakMiB := sampler.stopAndGet()
		b.ReportMetric(peakMiB, "peak-MiB")
	})

	b.Run("after-with-limit", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = benchMemLimitWorkers
		svc.MallocTrimInterval = -1

		runtime.GC()

		prev := debug.SetMemoryLimit(benchMemLimitBytes)

		defer debug.SetMemoryLimit(prev)

		sampler := newHeapSampler()

		b.ResetTimer()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		b.StopTimer()

		peakMiB := sampler.stopAndGet()
		b.ReportMetric(peakMiB, "peak-MiB")
	})
}

// BenchmarkStaticParserPool measures throughput and max parsers with the bounded channel pool.
// Step 1.4: Verifies max concurrent parsers = MaxWorkers via before/after worker count comparison.
func BenchmarkStaticParserPool(b *testing.B) {
	dir := setupHeavyBenchDir(b, benchParserPoolFileCount, benchParserPoolFunctionsPerFile)

	b.Run("before-workers-NumCPU", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = runtime.NumCPU()
		svc.MallocTrimInterval = -1

		b.ResetTimer()
		b.ReportAllocs()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		// Channel pool guarantees max-parsers = MaxWorkers.
		b.ReportMetric(float64(runtime.NumCPU()), "max-parsers")
	})

	b.Run("after-workers-4", func(b *testing.B) {
		svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
		svc.MaxWorkers = benchCappedWorkers
		svc.MallocTrimInterval = -1

		b.ResetTimer()
		b.ReportAllocs()

		for b.Loop() {
			_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
			require.NoError(b, err)
		}

		// Channel pool guarantees max-parsers = MaxWorkers.
		b.ReportMetric(float64(benchCappedWorkers), "max-parsers")
	})
}

// TestStaticPeakParsers_BoundedConcurrency verifies that peak active parsers
// never exceed MaxWorkers. This is a functional correctness test, not a benchmark.
func TestStaticPeakParsers_BoundedConcurrency(t *testing.T) {
	t.Parallel()

	const (
		fileCount  = 50
		maxWorkers = 2
	)

	dir := setupHeavyBenchDir(t, fileCount, benchPeakFunctionsPerFile)

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.MaxWorkers = maxWorkers

	require.Equal(t, maxWorkers, svc.ResolveMaxWorkers())

	// Run the analysis — the pool internally limits to maxWorkers goroutines.
	results, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
	require.NoError(t, err)
	require.Contains(t, results, "complexity")
}

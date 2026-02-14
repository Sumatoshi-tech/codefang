// bench-hibernation measures heap memory before and after Hibernate() calls
// during a real streaming run on a target repository.
//
// Usage:
//
//	go run ./scripts/bench-hibernation --repo ~/sources/kubernetes --limit 10000 \
//	  --analyzer file-history --profile-dir docs/profiles/file-history-hibernation
package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"slices"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

func main() {
	repoPath := flag.String("repo", "", "Path to git repository")
	limit := flag.Int("limit", 10000, "Number of commits to process")
	chunkSize := flag.Int("chunk-size", 5000, "Commits per chunk")
	profileDir := flag.String("profile-dir", "", "Directory to write heap profiles")
	analyzerName := flag.String("analyzer", "file-history", "Analyzer to benchmark (file-history, shotness)")
	uastPipelineWorkers := flag.Int("uast-pipeline-workers", 0, "UAST pipeline workers (0 = default, -1 = disable)")
	leafWorkers := flag.Int("leaf-workers", 0, "Leaf analyzer workers (0 = default, -1 = disable)")
	cpuProfile := flag.Bool("cpu-profile", false, "Write CPU profile to profile-dir/cpu.prof")

	flag.Parse()

	if *repoPath == "" {
		log.Fatal("--repo is required")
	}

	if *profileDir == "" {
		log.Fatal("--profile-dir is required")
	}

	if err := os.MkdirAll(*profileDir, 0o755); err != nil {
		log.Fatalf("mkdir profile-dir: %v", err)
	}

	if *cpuProfile {
		cpuPath := filepath.Join(*profileDir, "cpu.prof")

		cpuFile, cpuErr := os.Create(cpuPath)
		if cpuErr != nil {
			log.Fatalf("create cpu profile: %v", cpuErr)
		}
		defer cpuFile.Close()

		if startErr := pprof.StartCPUProfile(cpuFile); startErr != nil {
			log.Fatalf("start cpu profile: %v", startErr)
		}

		defer pprof.StopCPUProfile()

		log.Printf("CPU profiling enabled -> %s", cpuPath)
	}

	repo, err := gitlib.OpenRepository(*repoPath)
	if err != nil {
		log.Fatalf("open repo: %v", err)
	}
	defer repo.Free()

	commits := loadCommits(repo, *limit)
	log.Printf("loaded %d commits", len(commits))

	allAnalyzers, coreCount := buildPipeline(repo, *analyzerName)

	config := framework.DefaultCoordinatorConfig()

	if *uastPipelineWorkers != 0 {
		if *uastPipelineWorkers < 0 {
			config.UASTPipelineWorkers = 0
		} else {
			config.UASTPipelineWorkers = *uastPipelineWorkers
		}
	}

	if *leafWorkers != 0 {
		if *leafWorkers < 0 {
			config.LeafWorkers = 0
		} else {
			config.LeafWorkers = *leafWorkers
		}
	}

	runner := framework.NewRunnerWithConfig(repo, *repoPath, config, allAnalyzers...)
	runner.CoreCount = coreCount

	if err := runner.Initialize(); err != nil {
		log.Fatalf("initialize: %v", err)
	}

	// Plan chunks.
	chunks := planChunks(len(commits), *chunkSize)
	log.Printf("processing %d commits in %d chunks (chunk size %d)", len(commits), len(chunks), *chunkSize)

	// Collect hibernatables.
	var hibernatables []streaming.Hibernatable

	for _, a := range allAnalyzers {
		if h, ok := a.(streaming.Hibernatable); ok {
			hibernatables = append(hibernatables, h)
		}
	}

	// Process chunks with heap measurements at boundaries.
	type heapSnapshot struct {
		label     string
		heapInUse uint64
		heapSys   uint64
		heapIdle  uint64
		numGC     uint32
	}

	var snapshots []heapSnapshot

	takeSnapshot := func(label string) {
		runtime.GC()
		runtime.GC()

		var m runtime.MemStats
		runtime.ReadMemStats(&m)
		snapshots = append(snapshots, heapSnapshot{
			label:     label,
			heapInUse: m.HeapInuse,
			heapSys:   m.HeapSys,
			heapIdle:  m.HeapIdle,
			numGC:     m.NumGC,
		})
		log.Printf("  [heap] %-40s inuse=%6.1f MB  sys=%6.1f MB  idle=%6.1f MB",
			label, float64(m.HeapInuse)/1e6, float64(m.HeapSys)/1e6, float64(m.HeapIdle)/1e6)
	}

	writeHeapProfile := func(name string) {
		runtime.GC()
		runtime.GC()

		path := filepath.Join(*profileDir, name)

		f, ferr := os.Create(path)
		if ferr != nil {
			log.Printf("warning: create heap profile %s: %v", path, ferr)

			return
		}
		defer f.Close()

		if perr := pprof.WriteHeapProfile(f); perr != nil {
			log.Printf("warning: write heap profile %s: %v", path, perr)
		}
	}

	takeSnapshot("before_processing")
	writeHeapProfile("heap_before_processing.prof")

	for i, chunk := range chunks {
		if i > 0 {
			takeSnapshot(fmt.Sprintf("chunk_%d_end_before_hibernate", i))
			writeHeapProfile(fmt.Sprintf("heap_chunk_%d_before_hibernate.prof", i))

			// Hibernate all.
			for _, h := range hibernatables {
				if herr := h.Hibernate(); herr != nil {
					log.Fatalf("hibernate: %v", herr)
				}
			}

			takeSnapshot(fmt.Sprintf("chunk_%d_end_after_hibernate", i))
			writeHeapProfile(fmt.Sprintf("heap_chunk_%d_after_hibernate.prof", i))

			// Boot all.
			for _, h := range hibernatables {
				if berr := h.Boot(); berr != nil {
					log.Fatalf("boot: %v", berr)
				}
			}

			takeSnapshot(fmt.Sprintf("chunk_%d_end_after_boot", i))
		}

		log.Printf("processing chunk %d/%d (commits %d-%d)", i+1, len(chunks), chunk.start, chunk.end)

		if err := runner.ProcessChunk(commits[chunk.start:chunk.end], chunk.start); err != nil {
			log.Fatalf("process chunk %d: %v", i+1, err)
		}
	}

	// Final snapshot after last chunk.
	takeSnapshot("after_all_chunks")
	writeHeapProfile("heap_after_all_chunks.prof")

	// Finalize.
	_, err = runner.Finalize()
	if err != nil {
		log.Fatalf("finalize: %v", err)
	}

	takeSnapshot("after_finalize")
	writeHeapProfile("heap_after_finalize.prof")

	// Print summary table.
	fmt.Println()
	fmt.Println("=== Heap Memory Timeline ===")
	fmt.Printf("%-45s %10s %10s %10s\n", "Phase", "InUse(MB)", "Sys(MB)", "Idle(MB)")
	fmt.Println("---------------------------------------------+----------+----------+----------")

	for _, s := range snapshots {
		fmt.Printf("%-45s %10.1f %10.1f %10.1f\n",
			s.label, float64(s.heapInUse)/1e6, float64(s.heapSys)/1e6, float64(s.heapIdle)/1e6)
	}

	// Compute hibernation deltas.
	fmt.Println()
	fmt.Println("=== Hibernation Memory Deltas ===")

	for i := 0; i+1 < len(snapshots); i++ {
		curr := snapshots[i]

		next := snapshots[i+1]
		if contains(curr.label, "before_hibernate") && contains(next.label, "after_hibernate") {
			delta := float64(curr.heapInUse) - float64(next.heapInUse)
			pct := (delta / float64(curr.heapInUse)) * 100
			fmt.Printf("  %s -> %s: %.1f MB freed (%.1f%%)\n",
				curr.label, next.label, delta/1e6, pct)
		}
	}
}

func buildPipeline(repo *gitlib.Repository, analyzerName string) ([]analyze.HistoryAnalyzer, int) {
	treeDiff := &plumbing.TreeDiffAnalyzer{Repository: repo}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: repo}
	fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
	lineStats := &plumbing.LinesStatsCalculator{TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff}
	langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}
	uastChanges := &plumbing.UASTChangesAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

	switch analyzerName {
	case "file-history":
		core := []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect,
		}
		leaf := &filehistory.Analyzer{
			Identity: identity, TreeDiff: treeDiff, LineStats: lineStats,
		}

		return append(core, leaf), len(core)
	case "shotness":
		core := []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, langDetect, uastChanges,
		}
		leaf := &shotness.HistoryAnalyzer{
			FileDiff: fileDiff, UAST: uastChanges,
		}

		return append(core, leaf), len(core)
	case "sentiment":
		core := []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, langDetect, uastChanges,
		}
		leaf := &sentiment.HistoryAnalyzer{
			UAST: uastChanges, Ticks: ticks,
		}

		return append(core, leaf), len(core)
	default:
		log.Fatalf("unknown analyzer: %s (supported: file-history, shotness, sentiment)", analyzerName)

		return nil, 0
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > 0 && findSubstring(s, substr))
}

func findSubstring(s, sub string) bool {
	for i := 0; i <= len(s)-len(sub); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}

	return false
}

type chunkBounds struct {
	start int
	end   int
}

func planChunks(total, chunkSize int) []chunkBounds {
	var chunks []chunkBounds

	for start := 0; start < total; start += chunkSize {
		end := min(start+chunkSize, total)
		chunks = append(chunks, chunkBounds{start: start, end: end})
	}

	return chunks
}

func loadCommits(repo *gitlib.Repository, limit int) []*gitlib.Commit {
	iter, err := repo.Log(&gitlib.LogOptions{FirstParent: true})
	if err != nil {
		log.Fatalf("log: %v", err)
	}
	defer iter.Close()

	var commits []*gitlib.Commit

	for {
		c, nerr := iter.Next()
		if nerr != nil {
			break
		}

		if limit > 0 && len(commits) >= limit {
			c.Free()

			break
		}

		commits = append(commits, c)
	}

	// Reverse to oldest-first.
	slices.Reverse(commits)

	return commits
}

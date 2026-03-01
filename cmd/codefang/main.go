// Package main provides the entry point for the codefang CLI tool.
package main

import (
	"bufio"
	"fmt"
	"log"
	"net/http"
	nethttppprof "net/http/pprof"
	"os"
	"runtime"
	"runtime/pprof"
	"strings"
	"syscall"
	"time"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/cmd/codefang/commands"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// Memory watchdog and pprof configuration constants.
const (
	// watchdogInterval is the polling interval for the memory watchdog.
	watchdogInterval = 2 * time.Second

	// megabyte is 1 MiB in bytes, used for unit conversions.
	megabyte = 1024 * 1024

	// rssThresholdMiB is the RSS threshold in MiB above which heap dumps are triggered.
	rssThresholdMiB = 4096

	// pprofReadHeaderTimeout is the read header timeout for the pprof HTTP server.
	pprofReadHeaderTimeout = 10 * time.Second
)

var (
	verbose bool
	quiet   bool
)

// readRSSMiB reads current RSS from /proc/self/statm.
func readRSSMiB() int64 {
	f, err := os.Open("/proc/self/statm")
	if err != nil {
		return 0
	}
	defer f.Close()

	var vsize, rss int64

	_, scanErr := fmt.Fscan(f, &vsize)
	if scanErr != nil {
		return 0
	}

	_, scanErr = fmt.Fscan(f, &rss)
	if scanErr != nil {
		return 0
	}

	return rss * int64(os.Getpagesize()) / megabyte
}

// readProcField reads a named field from /proc/self/status.
func readProcField(field string) string {
	f, err := os.Open("/proc/self/status")
	if err != nil {
		return ""
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := scanner.Text()
		if after, ok := strings.CutPrefix(line, field); ok {
			return strings.TrimSpace(after)
		}
	}

	return ""
}

// readSmapsRollup reads /proc/self/smaps_rollup for memory region summary.
func readSmapsRollup() string {
	f, err := os.Open("/proc/self/smaps_rollup")
	if err != nil {
		return ""
	}
	defer f.Close()

	var sb strings.Builder

	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := scanner.Text()
		for _, prefix := range []string{
			"Rss:", "Pss:", "Anonymous:", "AnonHugePages:",
			"Shared_Clean:", "Shared_Dirty:",
			"Private_Clean:", "Private_Dirty:",
		} {
			if strings.HasPrefix(line, prefix) {
				sb.WriteString(line)
				sb.WriteByte(' ')
			}
		}
	}

	return sb.String()
}

// saveProcMaps copies /proc/self/maps to a file for offline analysis.
func saveProcMaps(path string) {
	src, err := os.Open("/proc/self/maps")
	if err != nil {
		return
	}
	defer src.Close()

	dst, err := os.Create(path)
	if err != nil {
		return
	}
	defer dst.Close()

	scanner := bufio.NewScanner(src)
	for scanner.Scan() {
		fmt.Fprintln(dst, scanner.Text())
	}
}

// handleRSSSpike dumps heap profile and /proc/self/maps when RSS exceeds threshold.
// Returns the updated dump count.
func handleRSSSpike(dumpCount int, rssMiB int64, dumpDir string) int {
	dumpCount++

	smaps := readSmapsRollup()
	log.Printf("SPIKE #%d: RSS=%d MiB smaps: %s", dumpCount, rssMiB, smaps)

	dumpHeapProfile(dumpDir, dumpCount, rssMiB)

	// Snapshot /proc/self/maps on first spike (runbook 5.6, 6.2).
	if dumpCount == 1 {
		saveProcMaps(fmt.Sprintf("%s/maps_spike_%dMiB.txt", dumpDir, rssMiB))
	}

	return dumpCount
}

// dumpHeapProfile writes a heap profile to the dump directory.
func dumpHeapProfile(dumpDir string, dumpCount int, rssMiB int64) {
	path := fmt.Sprintf("%s/heap_spike_%d_%dMiB.pb.gz", dumpDir, dumpCount, rssMiB)

	out, err := os.Create(path)
	if err != nil {
		return
	}
	defer out.Close()

	writeErr := pprof.Lookup("heap").WriteTo(out, 0)
	if writeErr != nil {
		log.Printf("heap profile write error: %v", writeErr)
	}
}

// startMemoryWatchdog per runbook sections 3.3 + 5.5 + 6.2:
//   - Logs RSS, GoHeap, GoSys, OS threads, goroutines, smaps every 2s (always)
//   - On threshold breach: dumps heap profile + /proc/self/maps snapshot
func startMemoryWatchdog(thresholdMiB int, dumpDir string) {
	go func() {
		dumpCount := 0
		tick := 0

		tickSeconds := int(watchdogInterval / time.Second)

		for {
			time.Sleep(watchdogInterval)

			tick++

			rssMiB := readRSSMiB()
			threads := readProcField("Threads:")

			var ms runtime.MemStats
			runtime.ReadMemStats(&ms)

			goHeapMiB := ms.HeapInuse / megabyte
			goSysMiB := ms.Sys / megabyte
			nativeMiB := rssMiB - int64(goSysMiB)
			goroutines := runtime.NumGoroutine()

			// Log every sample for time-series correlation (runbook 1.3, 3.3).
			log.Printf("MEM t=%d RSS=%d GoHeap=%d GoSys=%d Native=%d threads=%s goroutines=%d",
				tick*tickSeconds, rssMiB, goHeapMiB, goSysMiB, nativeMiB, threads, goroutines)

			if rssMiB > int64(thresholdMiB) && dumpCount < 5 {
				dumpCount = handleRSSSpike(dumpCount, rssMiB, dumpDir)
			}
		}
	}()

	// Save baseline maps at startup (runbook 6.2: compare t0 vs tN).
	saveProcMaps(fmt.Sprintf("%s/maps_baseline.txt", "/tmp"))
}

// ensureMallocTunables re-execs the process with ALL critical glibc malloc env
// vars set. glibc reads these at the very first malloc() call, before any
// threads exist. mallopt() called later from Go/CGO is too late.
//
// MALLOC_ARENA_MAX=2: limit to 2 arenas (default 8*cores = 192 on 24-core).
// MALLOC_MMAP_THRESHOLD_=32768: allocations >= 32 KiB use mmap → freed on free().
// MALLOC_TRIM_THRESHOLD_=16384: trim arenas aggressively.
// MALLOC_MMAP_MAX_=65536: allow many concurrent mmap regions.
//
// With MMAP_THRESHOLD=32K: tree-sitter parse trees (100 KiB - 10 MiB) and
// libgit2 objects bypass arenas entirely, preventing fragmentation that causes
// 2 arenas to grow to 20-45 GiB under concurrent CGO load.
func ensureMallocTunables() {
	if os.Getenv("MALLOC_ARENA_MAX") != "" {
		return // already configured (re-exec completed or manual override).
	}

	exe, err := os.Executable()
	if err != nil {
		return // best-effort; continue without tuning.
	}

	// Set all tunables before re-exec. glibc reads these at first malloc().
	os.Setenv("MALLOC_ARENA_MAX", "2")
	os.Setenv("MALLOC_MMAP_THRESHOLD_", "32768")
	os.Setenv("MALLOC_TRIM_THRESHOLD_", "16384")
	os.Setenv("MALLOC_MMAP_MAX_", "65536")

	// Re-exec replaces this process; does not return on success.
	execErr := syscall.Exec(exe, os.Args, os.Environ())
	if execErr != nil {
		log.Printf("re-exec failed: %v", execErr)
	}
}

func main() {
	ensureMallocTunables()

	// Start pprof HTTP server on localhost:6060 with explicit handler
	// registration (avoids gosec G108: DefaultServeMux exposure) and
	// read header timeout (avoids gosec G114: no timeouts).
	go func() {
		mux := http.NewServeMux()
		mux.HandleFunc("/debug/pprof/", nethttppprof.Index)
		mux.HandleFunc("/debug/pprof/cmdline", nethttppprof.Cmdline)
		mux.HandleFunc("/debug/pprof/profile", nethttppprof.Profile)
		mux.HandleFunc("/debug/pprof/symbol", nethttppprof.Symbol)
		mux.HandleFunc("/debug/pprof/trace", nethttppprof.Trace)
		server := &http.Server{
			Addr:              "localhost:6060",
			Handler:           mux,
			ReadHeaderTimeout: pprofReadHeaderTimeout,
		}
		log.Println(server.ListenAndServe())
	}()

	// Auto-dump heap when RSS exceeds 4 GiB.
	startMemoryWatchdog(rssThresholdMiB, "/tmp")

	version.InitBinaryVersion()

	rootCmd := &cobra.Command{
		Use:   "codefang",
		Short: "Codefang Code Analysis - Unified code analysis tool",
		Long: `Codefang provides comprehensive code analysis tools.

Commands:
  run       Unified static + history analysis entrypoint
  render    Render stored analysis results as multi-page HTML`,
		SilenceUsage:  true,
		SilenceErrors: true,
	}

	rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "verbose output")
	rootCmd.PersistentFlags().BoolVarP(&quiet, "quiet", "q", false, "suppress output")

	// Add commands.
	rootCmd.AddCommand(commands.NewRunCommand())
	rootCmd.AddCommand(commands.NewRenderCommand())
	rootCmd.AddCommand(versionCmd())

	err := rootCmd.Execute()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func versionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Show version information",
		Run: func(_ *cobra.Command, _ []string) {
			fmt.Fprintf(os.Stdout, "codefang %s (commit: %s, built: %s)\n", version.Version, version.Commit, version.Date)
		},
	}
}

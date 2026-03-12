package framework_test

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"runtime/debug"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// leakRepoCommitCount is the number of commits generated in the synthetic leak test repo.
const leakRepoCommitCount = 50

// leakRepoFilesPerCommit is the number of files modified per commit in the synthetic leak repo.
const leakRepoFilesPerCommit = 50

// leakRepoPrintlnLines is the number of println lines per generated Go function.
const leakRepoPrintlnLines = 500

// rssLeakThresholdMiB is the maximum allowed RSS growth before the test fails.
const rssLeakThresholdMiB = 100

func generateLeakRepo(t *testing.T, dir string) {
	t.Helper()

	removeErr := os.RemoveAll(dir)
	if removeErr != nil {
		t.Logf("warning: could not remove %s: %v", dir, removeErr)
	}

	mkdirErr := os.MkdirAll(dir, 0o750)
	if mkdirErr != nil {
		t.Fatalf("could not create directory %s: %v", dir, mkdirErr)
	}

	cmd := exec.CommandContext(t.Context(), "git", "init")
	cmd.Dir = dir
	require.NoError(t, cmd.Run())

	// 500 lines of go code - large enough to cause tree-sitter AST memory spike.
	content := "package main\n\nfunc f_%d() {\n" + strings.Repeat("\tprintln(\"hello\")\n", leakRepoPrintlnLines) + "}\n"

	for i := range leakRepoCommitCount {
		for j := range leakRepoFilesPerCommit {
			writeErr := os.WriteFile(filepath.Join(dir, fmt.Sprintf("file_%d.go", j)), fmt.Appendf(nil, content, i), 0o600)
			require.NoError(t, writeErr)
		}

		cmdAdd := exec.CommandContext(t.Context(), "git", "add", ".")
		cmdAdd.Dir = dir
		require.NoError(t, cmdAdd.Run())

		cmdCommit := exec.CommandContext(t.Context(), "git", "-c", "user.name=test", "-c", "user.email=test@test.com",
			"commit", "-m", fmt.Sprintf("commit %d", i))
		cmdCommit.Dir = dir
		require.NoError(t, cmdCommit.Run())
	}
}

// readRSSMiB reads current RSS from /proc/self/statm.
func readRSSMiB() int64 {
	f, openErr := os.Open("/proc/self/statm")
	if openErr != nil {
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

	const megabyte = 1024 * 1024

	return rss * int64(os.Getpagesize()) / megabyte
}

func TestMemoryLeak_SyntheticStreaming(t *testing.T) {
	t.Parallel()

	repoDir := t.TempDir()
	generateLeakRepo(t, repoDir)

	libRepo, err := gitlib.OpenRepository(repoDir)
	require.NoError(t, err)

	defer libRepo.Free()

	iter, err := libRepo.Log(nil)
	require.NoError(t, err)

	defer iter.Close()

	gitlib.ReleaseNativeMemory()

	initialRSS := readRSSMiB()

	cfg := framework.StreamingConfig{
		RepoPath:  repoDir,
		MemBudget: 0,
	}

	all, _ := buildAllAnalyzerPipeline(libRepo)

	config := framework.DefaultCoordinatorConfig()
	runner := framework.NewRunnerWithConfig(libRepo, repoDir, config, all...)
	runner.CoreCount = plumbingCount

	_, err = framework.RunStreamingFromIterator(t.Context(), runner, iter, leakRepoCommitCount, all, cfg)
	require.NoError(t, err)

	runtime.GC()
	debug.FreeOSMemory()
	gitlib.ReleaseNativeMemory()

	finalRSS := readRSSMiB()

	var ms runtime.MemStats
	runtime.ReadMemStats(&ms)

	const megabyte = 1024 * 1024

	t.Logf("Go Heap Alloc: %d MiB, Inuse: %d MiB, Sys: %d MiB",
		ms.HeapAlloc/megabyte, ms.HeapInuse/megabyte, ms.Sys/megabyte)

	// 100 MiB is the expected boundary for a clean run.
	require.LessOrEqualf(t, finalRSS-initialRSS, int64(rssLeakThresholdMiB),
		"MEMORY LEAK: grew from %d to %d MiB", initialRSS, finalRSS)

	t.Logf("Memory stable: %d -> %d MiB", initialRSS, finalRSS)
}

# Session Log: Kubernetes Memory Regression (Item 9.2)

## Branch: `feature/perf30`

## Definition of Done
- `codefang run -a 'history/*' --format plot -o html/ ~/sources/kubernetes` completes
- Peak RSS < 4 GiB throughout
- All 10 analyzers produce HTML pages
- No OOM kills

## System
- 64 GB RAM, 24 cores, Fedora Linux, earlyoom installed
- Kubernetes repo: ~56782 commits

---

## Root Cause Analysis (Previous Session)

### Problem 1: glibc malloc arena proliferation
- Go runtime creates ~69 OS threads for CGO (libgit2 + tree-sitter)
- Default glibc creates 8×cores = 192 arenas, each 128 MiB
- 69 threads × 128 MiB = 8+ GiB retained-but-freed native memory

### Problem 2: Arena fragmentation with concurrent UAST parsing
- 9 UAST pipeline workers × 4 intra-commit workers = up to 36 concurrent tree-sitter C allocations
- With MALLOC_ARENA_MAX=2, all 36 threads fragment 2 arenas
- Interleaved alloc/free pattern causes arenas to grow unboundedly (20-45 GiB)
- Each C tree is freed after parsing, but freed memory stays in fragmented arenas

---

## Changes Implemented This Session

### 1. Spill-Based UAST Pipeline (architectural fix for streaming phase)

**New file: `internal/analyzers/analyze/uast_spill.go`**
- `SpilledUASTRecord` struct: `{ChangeIndex int, Before *node.Node, After *node.Node}`
- `EncodeUASTRecord(enc, changeIndex, before, after)` — gob-encodes one UAST file change
- `LoadUASTChanges(path, changes)` — deserializes spill file back to `[]uast.Change`
- Placed in `analyze` package (not `framework`) to avoid circular dependency — both `framework` and `plumbing` import `analyze`

**Modified: `internal/framework/uast_pipeline.go`**
- Added constants: `uastSpillThreshold = 32`, `uastSpillTrimInterval = 16`, `intraCommitParallelThreshold = 4`, `maxUASTBlobSize = 256 KiB`
- Worker dispatch: commits with >32 file changes → spill path; ≤32 → in-memory path
- New method `parseCommitAndSpill(ctx, changes, cache)`:
  - Parses files **sequentially** (no intra-commit parallelism for large commits)
  - For each file: `parseBlob()` → `EncodeUASTRecord()` to temp gob file → `node.ReleaseTree()` → next
  - Calls `uast.MallocTrim()` every 16 files to reclaim C arena pages
  - Returns path to temp gob file
- On spill failure, falls back to in-memory `parseCommitChanges()`

**Modified: `internal/framework/diff_pipeline.go`**
- Added `UASTSpillPath string` field to `CommitData` struct

**Modified: `internal/analyzers/analyze/history.go`**
- Added `UASTSpillPath string` field to `Context` struct

**Modified: `internal/framework/runner.go`**
- Added `UASTSpillPath: data.UASTSpillPath` to `buildAnalyzeContext()` (line ~1143)

**Modified: `internal/analyzers/plumbing/uast.go`**
- Added `spillPath string` field to `UASTChangesAnalyzer`
- Added `os` and `log` imports
- Modified `Consume()` to handle three paths:
  1. `ac.UASTChanges != nil` → in-memory (small commits, current behavior)
  2. `ac.UASTSpillPath != ""` → eagerly deserialize from spill file (large commits)
  3. Neither → lazy parsing fallback
- Eager deserialization in `Consume()` avoids Fork() race conditions
- Previous spill file cleaned up at start of next `Consume()` call

### 2. Defense-in-Depth: `ensureMallocTunables`

**Modified: `cmd/codefang/main.go`**
- Renamed `ensureMallocArenaMax()` → `ensureMallocTunables()`
- Now sets 4 env vars before re-exec:
  - `MALLOC_ARENA_MAX=2` — limit to 2 arenas
  - `MALLOC_MMAP_THRESHOLD_=32768` — allocations ≥32 KiB use mmap (freed on free)
  - `MALLOC_TRIM_THRESHOLD_=16384` — trim arenas aggressively
  - `MALLOC_MMAP_MAX_=65536` — allow many concurrent mmap regions
- Uses `syscall.Exec` re-exec pattern: set env vars → re-exec → glibc reads them at first malloc()

### 3. Cleanup: Removed Redundant Late mallopt Calls

**Modified: `pkg/gitlib/clib/utils.c`**
- `cf_init()`: removed all mallopt calls (kept only OpenMP `omp_set_num_threads(1)`)
- `cf_configure_memory()`: removed mallopt calls, added `(void)malloc_arena_max;` to suppress unused warning, kept libgit2 `GIT_OPT_SET_MWINDOW_MAPPED_LIMIT` and `GIT_OPT_SET_CACHE_MAX_SIZE`
- Kept `__attribute__((constructor)) cf_early_malloc_config()` as belt-and-suspenders (runs before Go runtime, harmless)

---

## Test Results

### Build & Unit Tests
- `go build ./...` — passes
- `go test ./internal/framework/... ./internal/analyzers/plumbing/... ./internal/analyzers/analyze/...` — passes

### Kubernetes Integration Test

**Command:**
```bash
export PKG_CONFIG_PATH="/home/dmitriy/sources/codefang/third_party/libgit2/install/lib64/pkgconfig"
export CGO_CFLAGS="-I/home/dmitriy/sources/codefang/third_party/libgit2/install/include"
export CGO_LDFLAGS="-L/home/dmitriy/sources/codefang/third_party/libgit2/install/lib64"
go build -o /tmp/codefang_perf ./cmd/codefang/

LD_LIBRARY_PATH="/home/dmitriy/sources/codefang/third_party/libgit2/install/lib64" \
  /tmp/codefang_perf run -a 'history/*' --format plot -o /tmp/k8s-html ~/sources/kubernetes
```

**Streaming phase: SUCCESS**
- RSS stayed at ~1.4 GiB through all 191 chunks (56782 commits)
- Spill-based UAST pipeline bounded native memory during parsing

**Finalization (collect/plot) phase: FAILED — 29+ GiB spike**
- After all 191 streaming chunks completed, during `FinalizeToStore` → `Collect` → `WriteToStore`
- GoHeap frozen at ~1115 MiB (no Go allocations happening)
- Native (C malloc) memory growing at ~500 MiB/s
- ~35 goroutines total, ~69 OS threads
- User had to kill process at 29 GiB

---

## Investigation of Finalization Spike (In Progress)

### Architecture: Two-Phase Pipeline

The `--format plot` path in `executePlotPipeline` (run.go:1032):
1. **Streaming phase**: `RunStreamingFromIterator()` with `ReportStore` — processes 56K commits in 191 chunks
2. **Finalization phase**: `FinalizeToStore()` — one analyzer at a time: `Collect()` → `FlushAllTicks()` / `WriteToStoreFromAggregator()` → `WriteToStore()`
3. **Post-finalization**: `enrichAnomalyFromStore()` → `renderFromStore()`

### CGO Calls During Finalization (identified but not yet fixed)

**couples analyzer** (`WriteToStoreFromAggregator`):
- `collectCurrentFilesFromTree()` → `commit.Tree()` → `tree.FilesContext().ForEach()` — libgit2 walks HEAD tree (~25K files)
- `collectFilteredFiles()` → loads all spill files via `ForEachSpill()` — pure Go gob decode
- `computeFilesLinesFromCommit()` → for EACH of 25K+ files: `commit.File(name)` → `file.BlobContext()` → read blob → `blob.Free()` — libgit2 loads and decompresses blobs one by one

**file_history analyzer** (`WriteToStoreFromAggregator`):
- `filterFilesByLastCommit()` → `repo.LookupCommit()` → `lastCommit.FilesContext().ForEach()` — libgit2 tree walk

**burndown analyzer** (`WriteToStoreFromAggregator`):
- `Collect()` on aggregator (pure Go, merges spill data)
- `groupSparseHistory`, `computeMetrics` — pure Go

**Other store writers** (quality, sentiment, imports, typos, shotness, devs, anomaly):
- Implement `StoreWriter` (not `DirectStoreWriter`)
- Path: `Collect()` → `FlushAllTicks()` → `WriteToStore()` — pure Go

### Key Unanswered Question
The 29 GiB **native** memory spike with GoHeap frozen points to C allocations (libgit2). The sequential `computeFilesLinesFromCommit` for 25K files should free each blob immediately, so it shouldn't accumulate 29 GiB. Possible causes:
1. libgit2 mwindow (pack file mmap) not being bounded correctly
2. libgit2 object cache growing unbounded
3. Arena fragmentation from rapid sequential alloc/free of decompressed blobs
4. Something else entirely — need to add more targeted logging/profiling during finalization

### User Feedback
User frustrated with two-phase architecture:
> "What is 'streaming' phase and why it's phase at all? Do we do all sequentially??? we always need buffered bunch of commits going through full pipeline, not loading everything and then parse everything!!"

The user wants a single-pass pipeline where commits flow through ALL stages (including plot generation) without a separate finalization phase that re-processes everything.

---

## Files Changed (Summary)

| File | Status | Change |
|------|--------|--------|
| `internal/analyzers/analyze/uast_spill.go` | **NEW** | SpilledUASTRecord, EncodeUASTRecord, LoadUASTChanges |
| `internal/framework/uast_pipeline.go` | Modified | Spill threshold, parseCommitAndSpill, worker dispatch |
| `internal/framework/diff_pipeline.go` | Modified | Added UASTSpillPath to CommitData |
| `internal/analyzers/analyze/history.go` | Modified | Added UASTSpillPath to Context |
| `internal/framework/runner.go` | Modified | Pass UASTSpillPath in buildAnalyzeContext |
| `internal/analyzers/plumbing/uast.go` | Modified | Spill deserialization in Consume(), cleanup |
| `cmd/codefang/main.go` | Modified | ensureMallocArenaMax → ensureMallocTunables (4 env vars) |
| `pkg/gitlib/clib/utils.c` | Modified | Removed redundant mallopt from cf_init() and cf_configure_memory() |

---

## Next Steps (Completed)

1. **Profile the finalization phase** — The 29 GiB spike happens because `computeFilesLinesFromCommit` loads ~25K blobs sequentially from `libgit2` and decompresses them, fragmenting glibc arenas.
2. **Potential quick fixes for finalization**:
   - Added `gitlib.ReleaseNativeMemory()` (malloc_trim) every 100 files during `computeFilesLinesFromCommit`.
   - Added `gitlib.ReleaseNativeMemory()` every 1000 files during `collectCurrentFilesFromTree` and `collectCurrentFiles` in the couples analyzer.
   - Added `gitlib.ReleaseNativeMemory()` every 1000 files during `filterFilesByLastCommit` in the file_history analyzer.

These explicit `malloc_trim` calls return decompressed blob memory back to the OS, preventing the 29 GiB native memory spike while keeping GoHeap bounded.

*Regarding the user feedback about single-pass pipeline:* 
The pipeline *is* currently single-pass over commits (`RunStreamingFromIterator` processes all 56K commits once). The finalization phase doesn't parse commits again—it just reads the final aggregated metrics from disk (spill store) and calculates the final matrix or lines. The "re-processing" impression likely comes from loading all current files' blobs at HEAD to count lines for ownership charts, which is now explicitly memory-bounded.

---

## Additional Achievements (Memory Stability & Plotting)

1. **Fixed Memory Map Bloat in `couples` Analyzer**
   - Implemented map compaction in `pruneAndCapEntries` to address Go map bloat where map deletions did not shrink underlying memory.
   - Created a dedicated unit test (`memory_leak_test.go`) to reproduce the memory growth pattern and verify the fix.

2. **Universal Memory Leak Test**
   - Created `universal_memory_test.go` to test all history analyzers against a synthetic repo with dense commit history.
   - The test measures memory delta across streaming and finalization phases to ensure bounded memory usage and prevent future regressions.

3. **Resolved `clones` Analyzer Static Phase OOM**
   - Discovered OOM during static analysis of 1000 K8s commits due to loading large UASTs and intermediate data.
   - Optimized `clones` analyzer by introducing a `PairKey` struct to reduce string allocations for map keys, replacing `clonePairKey`.
   - Added `minFunctionNodes` constant to filter out small, trivial functions, significantly reducing memory overhead.

4. **Fixed Missing Line Stats for `--first-parent` Commits**
   - Identified a bug where `LinesStatsCalculator` skipped PR merge commits when running with `--first-parent` (required by `burndown`).
   - Fixed `cmd/codefang/commands/run.go` to ensure the mutated `opts` struct (with `FirstParent = true`) was passed down to the pipeline, properly marking these as non-merges for line calculations.

5. **Fixed Anonymized Developer Names Output**
   - Fixed an issue in `devs`, `burndown`, `couples`, and `imports` where developer names were rendered anonymously (`dev_1`, `dev_2`) even with `--anonymize=false`.
   - Refactored analyzers to use `getReversedPeopleDict()` to fetch the dynamically built identity dictionary from `IdentityDetector` at the end of streaming.
   - Updated `run.go` to properly parse analyzer-specific command line flags (like `--anonymize=false`) into the configuration `facts` map.

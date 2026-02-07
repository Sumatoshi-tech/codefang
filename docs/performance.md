# Performance-Driven Design

Codefang is built around two performance goals:

1. Maximize throughput on large histories by minimizing CGO overhead and I/O cost.
2. Control memory pressure and GC overhead while keeping hot data CPU-cache friendly.

This document explains the arena pattern used for blob loading and how it fits into
the broader performance-driven design.

## The Arena Pattern (Blob Loading)

### Why an arena?
Batch blob loading often allocates many small buffers, which creates GC pressure
and fragments memory. Instead, Codefang uses a single contiguous byte arena per
batch request and slices into it for each blob. This keeps allocations predictable
and improves cache locality.

### Where it is used
The arena is used in the history pipeline when loading blob contents:

- `pkg/framework/BlobPipeline` allocates an arena per batch request.
- `pkg/gitlib/Worker` uses the arena-aware CGO path.
- `pkg/gitlib/CGOBridge` calls the C loader to fill the arena and returns slices.

### Lifecycle and data flow
1. `BlobPipeline` allocates a `[]byte` arena per batch shard.
2. The arena is passed to the `BlobBatchRequest`.
3. The worker executes a single CGO call that fills the arena.
4. Each result returns an offset and size, and Go slices are created over the arena.
5. If the arena is too small for a blob, the worker falls back to per-blob loading.

```232:241:pkg/framework/blob_pipeline.go
		// Allocate arena for this batch
		// We allocate one arena per request. It will be passed to CGO to fill.
		arena := make([]byte, p.ArenaSize)

		req := gitlib.BlobBatchRequest{
			Hashes: chunk,
			Arena:  arena,
		}
```

```102:185:pkg/gitlib/cgo_bridge.go
// BatchLoadBlobsArena loads multiple blobs into a provided arena.
// It uses internal recycled buffers for C requests/results to avoid allocation.
func (b *CGOBridge) BatchLoadBlobsArena(hashes []Hash, arena []byte) []BlobResult {
	// ...
	C.cf_batch_load_blobs_arena(
		(*C.git_repository)(repoPtr),
		&b.requestBuf[0],
		C.int(count),
		arenaPtr,
		C.size_t(len(arena)),
		&b.resultBuf[0],
	)
	// ...
}
```

```168:204:pkg/gitlib/worker.go
	case BlobBatchRequest:
		// Use Arena loading if provided (zero-copy efficiency)
		if typedReq.Arena != nil {
			results = w.bridge.BatchLoadBlobsArena(typedReq.Hashes, typedReq.Arena)
			// Handle arena overflow by falling back to standard load
			for i := range results {
				if errors.Is(results[i].Error, ErrArenaFull) {
					// Fallback to standard allocation for this single blob
					fallbackRes := w.bridge.BatchLoadBlobs([]Hash{results[i].Hash})
					if len(fallbackRes) == 1 {
						results[i] = fallbackRes[0]
					}
				}
			}
		} else {
			results = w.bridge.BatchLoadBlobs(typedReq.Hashes)
		}
```

### Memory safety and GC behavior
- The arena is Go-owned memory; CGO only writes into it.
- `runtime.Pinner` is used to pin the arena and result buffers during CGO calls.
- `CachedBlob` slices reference the arena, so arenas live as long as the blobs do.

When a blob is inserted into a global cache, it is cloned to detach from the
arena and avoid keeping large arenas alive longer than necessary.

```90:108:pkg/framework/blob_cache.go
	// Add new entry
	// Clone the blob to ensure data is detached from any large arena
	safeBlob := blob.Clone()
```

### Configuration and tuning
The arena size is configurable via `CoordinatorConfig.BlobArenaSize`.

Defaults:
- `DefaultBlobBatchArenaSize`: 4 MB
- `DefaultCoordinatorConfig.BlobArenaSize`: 4 MB

Choosing a size:
- Larger arenas reduce fallback frequency but increase peak memory usage.
- Smaller arenas reduce GC pressure but can cause more per-blob fallbacks.

## Broader Performance-Driven Design

### 1) Batch CGO operations
To reduce CGO call overhead, Codefang batches blob loading and diffing:
- `BatchLoadBlobs` and `BatchLoadBlobsArena` load multiple blobs per call.
- `BatchDiffBlobs` computes multiple diffs per call.

### 2) Pack-aware ODB access
The C layer sorts OIDs and uses direct ODB reads to maximize pack cache locality
and avoid repeated lookups.

### 3) Parallelism without libgit2 contention
The worker pool uses separate repository handles and locks each worker to a
single OS thread, which is required by libgit2 and keeps concurrent CGO calls safe.

### 4) Cache-first pipelines
Blob and diff pipelines are fronted by LRU caches to avoid recomputing data:
- `GlobalBlobCache` caches `CachedBlob` by hash.
- `DiffCache` caches diff results by hash pairs.

### 5) Single-pass UAST traversal
Static analyzers share a single UAST traversal, reducing AST walks from N
per analyzer to one pass for a group of analyzers.

### 6) Memory-dense core data structures
Large history analyzers use contiguous, index-based structures instead of
pointer-heavy nodes to reduce GC overhead and improve cache locality.

## CLI Resource Knobs

The `codefang history` command exposes flags to tune pipeline performance for
large repositories. These flags allow operators to manually control memory and
concurrency settings.

### Available Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--workers` | int | CPU count | Number of parallel workers |
| `--buffer-size` | int | workers×2 | Size of internal pipeline channels |
| `--commit-batch-size` | int | 100 | Commits per processing batch |
| `--blob-cache-size` | size | 1GiB | Max blob cache size |
| `--diff-cache-size` | int | 10000 | Max diff cache entries |
| `--blob-arena-size` | size | 4MiB | Memory arena size for blob loading |
| `--memory-budget` | size | - | Memory budget for auto-tuning all knobs |

### Memory Budget Auto-Tuning

Instead of manually configuring each knob, you can use `--memory-budget` to let
Codefang automatically derive optimal settings based on a target memory limit:

```bash
codefang history --memory-budget=512MiB -a burndown .
```

The budget controller uses a cost model to allocate memory:
- **60%** to caches (blob cache + diff cache)
- **30%** to workers (repository handles + arenas)
- **10%** to buffers (in-flight data)
- **5%** slack for runtime overhead

**Budget → Derived Knobs Examples:**

| Budget | Workers | Blob Cache | Diff Cache | Buffer Size |
|--------|---------|------------|------------|-------------|
| 256 MiB | 1-2 | ~90 MiB | ~2,300 | 2-4 |
| 512 MiB | 2-4 | ~200 MiB | ~5,000 | 4-8 |
| 1 GiB | 4-8 | ~400 MiB | ~10,000 | 8-16 |
| 2 GiB | 8-16 | ~850 MiB | ~22,000 | 16-32 |

The minimum budget is 128 MiB. Budgets below this threshold will result in an error.

### Size Format

Size flags accept human-readable values using the `go-humanize` format:
- SI units: `MB`, `GB` (1000-based)
- Binary units: `MiB`, `GiB` (1024-based)

Examples: `256MiB`, `1GiB`, `500MB`

## Streaming Chunk Processing

Codefang always processes commits in streaming chunks. This improves performance
by keeping the working set small enough for CPU caches, reducing libgit2 pack
cache contention, and bounding memory growth.

### How It Works

1. Commits are split into chunks (2,000-5,000 commits each)
2. Each chunk is processed independently through the pipeline
3. Between chunks, analyzers hibernate to release temporary state
4. Results are aggregated across all chunks

```bash
codefang history --memory-budget=256MiB -a burndown .

# Output shows chunk processing
# streaming: processing 100000 commits in 20 chunks
# streaming: processing chunk 1/20 (commits 0-5000)
# streaming: processing chunk 2/20 (commits 5000-10000)
# ...
```

### Chunk Size

The default chunk size is 5,000 commits (empirically optimal for CPU cache locality).
When `--memory-budget` is set, the chunk size may be reduced:
- Available memory after overhead is divided by per-commit state growth (~2 KiB)
- Minimum chunk size: 2,000 commits
- Maximum chunk size: 5,000 commits

## Checkpointing for Crash Recovery

Codefang saves checkpoints after each streaming chunk by default. If the process
crashes or is interrupted, the analysis resumes from the last checkpoint instead
of starting over.

### Checkpoint Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--checkpoint` | bool | true | Enable checkpointing |
| `--checkpoint-dir` | string | ~/.codefang/checkpoints | Directory for checkpoint files |
| `--resume` | bool | true | Resume from checkpoint if available |
| `--clear-checkpoint` | bool | false | Clear existing checkpoint before run |

### How It Works

When checkpointing is enabled (default):
1. After each chunk completes, analyzer state is saved to disk
2. Progress metadata (current chunk, commit hash) is recorded
3. If the process crashes, the next run detects the checkpoint
4. Analysis resumes from the chunk after the last saved checkpoint
5. On successful completion, the checkpoint is cleared

### Storage Location

Checkpoints are stored in `~/.codefang/checkpoints/<repo-hash>/`:
- `checkpoint.json` - Metadata (human-readable)
- `analyzer_N/` - Per-analyzer state files

The `<repo-hash>` is derived from the repository path, ensuring different
repositories don't conflict.

### Example Usage

```bash
# Normal run with checkpointing (enabled by default)
codefang history -a burndown ~/sources/kubernetes

# Disable checkpointing
codefang history --checkpoint=false -a burndown ~/sources/kubernetes

# Force fresh start by clearing checkpoint
codefang history --clear-checkpoint -a burndown ~/sources/kubernetes

# Use custom checkpoint directory
codefang history --checkpoint-dir=/tmp/cp -a burndown ~/sources/kubernetes
```

### Checkpoint Validation

When resuming, Codefang validates that the checkpoint matches the current run:
- Repository path must match
- Analyzer set must match

If validation fails, the checkpoint is ignored and analysis starts fresh.

### Checkpoint Best Practices

- Leave checkpointing enabled for large repository analysis
- Use `--clear-checkpoint` when changing analyzers or analysis parameters
- Checkpoints are automatically cleared after successful completion
- Monitor disk space if analyzing many large repositories

### Analyzer Checkpoint Support

All history analyzers support checkpointing and hibernation:

| Analyzer | Checkpoint Support | Hibernation Support | Serialization Format |
|----------|-------------------|---------------------|---------------------|
| burndown | Yes | Yes (compacts file timelines) | gob (binary) |
| devs | Yes | Yes (clears merges) | JSON |
| couples | Yes | Yes (clears merges) | JSON |
| file-history | Yes | Yes (clears merges) | JSON |
| imports | Yes | Yes (releases parser) | JSON |
| sentiment | Yes | Yes (no-op) | JSON |
| shotness | Yes | Yes (clears merges) | JSON |
| typos | Yes | Yes (clears levenshtein context) | JSON |

All analyzers support crash recovery when checkpointing is enabled.

### Example: Large Repository Preset

For repositories with 100k+ commits, consider reducing memory pressure:

```bash
codefang history \
  --workers=8 \
  --buffer-size=16 \
  --commit-batch-size=25 \
  --blob-cache-size=128MiB \
  --diff-cache-size=2000 \
  --blob-arena-size=4MiB \
  -a burndown .
```

### Example: Memory-Constrained Environment

For systems with limited RAM:

```bash
codefang history \
  --workers=4 \
  --blob-cache-size=64MiB \
  --diff-cache-size=1000 \
  -a devs .
```

## Operational Guidance

- Increase `--blob-cache-size` when analyzing large repositories with high blob reuse.
- Increase `--diff-cache-size` when commits repeatedly touch the same files.
- Increase `--blob-arena-size` when large files are common and fallbacks are frequent.
- Reduce `--workers` if CPU contention outweighs parallel gains.
- Use `--commit-batch-size` to control memory per batch (smaller = less memory, more batches).

## Battle Test: kubernetes Repository

### Environment

| Item | Value |
|------|-------|
| CPU | AMD Ryzen AI 9 HX 370 (24 logical cores) |
| RAM | 64 GB DDR5 |
| OS | Linux 6.18.7-200.fc43.x86_64 (Fedora 43) |
| Repository | kubernetes (135,104 total / 56,782 first-parent commits) |
| Analyzer | burndown (--first-parent, streaming auto, checkpoint on) |
| Date | 2026-02-07 |

### Results

| Metric | Value |
|--------|-------|
| Wall clock | 1m 56.6s |
| User time | 954.5s |
| System time | 71.3s |
| CPU utilization | 879% |
| Peak RSS | 15.2 GB |
| Major page faults | 0 |
| Minor page faults | 12.2M |
| Voluntary context switches | 17.8M |
| Involuntary context switches | 173k |
| Streaming chunks | 12 (5,000 commits each) |

### USE Analysis (Brendan Gregg's Methodology)

The USE method examines **Utilization**, **Saturation**, and **Errors** for each
resource to identify bottlenecks systematically.

#### CPU

| Dimension | Observation |
|-----------|-------------|
| **Utilization** | 879% across 24 cores (37% per-core average). 14 workers active (60% of cores). |
| **Saturation** | 173k involuntary context switches indicate moderate scheduling pressure. Worker threads block on CGO calls (69% of CPU time in `runtime.cgocall`). |
| **Errors** | None. |

**Key finding:** 69% of CPU time is spent in CGO calls — 48% in `TreeDiff` (C-level
tree diffing) and 14% in `BatchLoadBlobsArena` (blob loading). The Go-side burndown
analyzer consumes ~6% (treap operations). GC accounts for 2.9% (`gcBgMarkWorker`).

#### Memory

| Dimension | Observation |
|-----------|-------------|
| **Utilization** | 15.2 GB peak RSS out of 64 GB available (24%). |
| **Saturation** | 12.2M minor page faults (normal for large working set). Zero major faults — all data served from RAM. |
| **Errors** | None. No OOM events. |

**Heap profile (inuse at exit):** Only 25 MB in-use after final GC — streaming mode
and hibernation effectively release inter-chunk state. The 15.2 GB peak is dominated
by transient allocations during chunk processing.

**Allocation profile (cumulative):** 134.6 GB total allocated over the run:
- 51% — `burndown.ensureCapacity` (treap node arrays, expected for line tracking)
- 24% — `BlobPipeline.processBatch` (arena allocations, recycled per batch)
- 16% — `CachedBlob.Clone` (blob copies for cache insertion)
- 6% — `BatchLoadBlobs` (fallback path when arena overflows)

#### I/O

| Dimension | Observation |
|-----------|-------------|
| **Utilization** | 243 MB written (profiles + output). Zero filesystem reads (git packfiles are mmap'd). |
| **Saturation** | No I/O wait observed (0 major page faults). |
| **Errors** | None. |

The libgit2 ODB uses mmap for pack file access, keeping I/O off the critical path.

#### Caches

| Resource | Observation |
|----------|-------------|
| **Blob cache** | 1 GB default. Arena fallback path (`BatchLoadBlobs`) consumed 6% of total allocations, indicating the 4 MB arena handles most blobs successfully. |
| **Diff cache** | 10,000 entries default. Diff operations account for only 2% of CPU time (`BatchDiffBlobs`), suggesting good cache reuse. |

### CPU Flamegraph Observations

The CPU flamegraph (`profiles/kubernetes/*/cpu_flamegraph.svg`) shows:

1. **Widest bar:** `runtime.cgocall` → `cf_tree_diff` (48%). This is the C-level
   git tree comparison — the fundamental cost of history analysis.
2. **Second widest:** `cf_batch_load_blobs_arena` (14%). Blob loading from packfiles.
3. **Go-side hotspot:** `treapTimeline.splitByLines` (4%). The burndown analyzer's
   persistent data structure operations.
4. **GC pressure:** `gcBgMarkWorker` at 2.9% — low, confirming the arena pattern
   effectively reduces GC overhead.
5. **Memory copies:** `runtime.memmove` at 3% — driven by blob cloning for cache.

### Heap Profile Observations

At-rest heap after GC shows 25 MB — dominated by static init data (language detection
maps from `enry`). The streaming architecture successfully reclaims all per-chunk state.

The cumulative allocation profile reveals that `burndown.ensureCapacity` is the largest
allocator (69 GB over the run). This is the treap node array growth for line-level
tracking. The arena pattern keeps `BlobPipeline` allocations efficient (32 GB allocated
but reused across batches).

## Battle Test: Streaming vs Non-Streaming on kubernetes

### Environment

Same hardware as the battle test above. Burndown analyzer, `--first-parent`,
all 56,782 first-parent commits. Each mode run with CPU and heap profiling.

### Results

| Metric | streaming=OFF | streaming=ON | Delta |
|--------|--------------|-------------|-------|
| Wall clock | 9m 17.2s | 1m 38.3s | **5.7x faster** |
| User time | 2621.7s | 827.8s | 3.2x less |
| System time | 811.2s | 67.5s | **12x less** |
| CPU utilization | 616% | 910% | +48% higher |
| Peak RSS | 14.4 GB | 15.7 GB | +9% |
| Minor page faults | 7.9M | 11.2M | +42% |
| Voluntary ctx switches | 400.6M | 18.2M | **22x fewer** |
| Involuntary ctx switches | 596k | 224k | 2.7x fewer |
| **Output** | **byte-identical** | **byte-identical** | **PASS** |

### Output Correctness

The YAML reports from both modes are byte-identical, confirming that the streaming
orchestrator with hibernate/boot cycles produces the same results as a single-pass run.

### Per-Chunk Timing (streaming=ON, 12 chunks of 5,000 commits)

| Chunk | Commits | Wall Time | Notes |
|-------|---------|-----------|-------|
| 1 | 0-5,000 | 2s | Earliest commits, small changes |
| 2 | 5,000-10,000 | 4s | |
| 3 | 10,000-15,000 | 9s | |
| 4 | 15,000-20,000 | 9s | |
| 5 | 20,000-25,000 | 8s | |
| 6 | 25,000-30,000 | 7s | |
| 7 | 30,000-35,000 | 8s | |
| 8 | 35,000-40,000 | 8s | |
| 9 | 40,000-45,000 | 11s | Larger merges in recent history |
| 10 | 45,000-50,000 | 15s | |
| 11 | 50,000-55,000 | 12s | |
| 12 | 55,000-56,782 | 5s | Partial chunk (1,782 commits) |

Later chunks take longer because recent kubernetes commits touch more files per commit.

### USE Analysis: Streaming Overhead

#### CPU

| Dimension | streaming=OFF | streaming=ON |
|-----------|--------------|-------------|
| **Utilization** | 616% (26% per-core) | 910% (38% per-core) |
| **Saturation** | 400M voluntary ctx switches — severe contention on libgit2 pack cache with unbounded working set | 18M voluntary ctx switches — chunked working set fits better in CPU cache |
| **Errors** | None | None |

Streaming mode achieves **higher CPU utilization** (910% vs 616%) because smaller working
sets reduce cache thrashing and libgit2 pack cache contention. The 22x reduction in
voluntary context switches confirms that the non-streaming mode spends most of its time
blocked on contention rather than doing useful work.

#### Memory

| Dimension | streaming=OFF | streaming=ON |
|-----------|--------------|-------------|
| **Utilization** | 14.4 GB peak | 15.7 GB peak (+9%) |
| **Saturation** | 811s system time — heavy mmap/munmap pressure from unbounded pack traversal | 67.5s system time — bounded working set |
| **Errors** | None | None |

The 12x reduction in system time is the most striking finding. Non-streaming mode's
unbounded commit traversal causes excessive mmap activity in libgit2's pack file access.
Streaming bounds the working set per chunk, dramatically reducing kernel overhead.

#### I/O

| Dimension | streaming=OFF | streaming=ON |
|-----------|--------------|-------------|
| **Utilization** | 912 KB written | 243 MB written (checkpoint files) |
| **Saturation** | None | None |
| **Errors** | None | None |

Streaming writes checkpoint files between chunks (243 MB total), but this has negligible
impact — the I/O is not on the critical path.

### Key Insight

The streaming mode's 5.7x speedup is **not** from reduced memory — RSS is similar.
It comes from **better memory locality**. Processing 5,000 commits at a time keeps the
working set (git pack indices, blob cache entries, treap nodes) small enough to fit in
CPU caches. The non-streaming mode traverses the entire 56k commit history with an
ever-growing working set, causing catastrophic cache thrashing and libgit2 contention
(evidenced by 400M voluntary context switches and 811s of system time).

This validates the streaming chunk orchestrator as a significant performance optimization,
not just a memory management tool.

## Summary

The arena pattern is one piece of a broader design strategy:
reduce CGO overhead, keep memory contiguous, avoid GC churn, and reuse work
through caches and batch operations. The result is stable throughput on
large repositories without sacrificing correctness.

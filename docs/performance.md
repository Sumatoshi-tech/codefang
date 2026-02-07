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

## Streaming Mode for Large Repositories

For repositories with very large commit histories (50,000+ commits), Codefang
can automatically process commits in streaming chunks to bound memory usage.

### Streaming Mode Flag

| Flag | Values | Default | Description |
|------|--------|---------|-------------|
| `--streaming-mode` | auto, on, off | auto | Control streaming chunk processing |

- **auto**: Detect large repos and enable streaming automatically
- **on**: Force streaming mode regardless of repo size
- **off**: Disable streaming (may OOM on very large repos)

### When Streaming Mode Activates (auto)

In `auto` mode, streaming is enabled when either:
1. The repository has 50,000+ commits
2. A memory budget is set and estimated peak memory exceeds 80% of the budget

### How It Works

When streaming mode is active:
1. Commits are split into chunks (10,000-50,000 commits each)
2. Each chunk is processed independently
3. Between chunks, analyzers may hibernate to reduce memory
4. Results are aggregated across all chunks

```bash
# Force streaming mode for testing
codefang history --streaming-mode=on --memory-budget=256MiB -a burndown .

# Output shows chunk processing
# streaming: processing 100000 commits in 3 chunks
# streaming: processing chunk 1/3 (commits 0-40000)
# streaming: processing chunk 2/3 (commits 40000-80000)
# streaming: processing chunk 3/3 (commits 80000-100000)
```

### Chunk Size Calculation

The chunk size is determined by the memory budget:
- Available memory after overhead is divided by per-commit state growth (~2 KiB)
- Minimum chunk size: 10,000 commits (to amortize hibernation cost)
- Maximum chunk size: 50,000 commits (to bound memory growth)

| Budget | Approximate Chunk Size |
|--------|----------------------|
| 128 MiB | ~40,000 commits |
| 256 MiB | 50,000 commits (max) |
| 512 MiB | 50,000 commits (max) |

### Streaming Mode Best Practices

- Use `--memory-budget` with streaming mode for predictable memory usage
- For very large repos (100k+ commits), streaming is recommended
- Streaming adds ~5-10% overhead due to chunk transitions
- Output is identical to non-streaming mode

## Checkpointing for Crash Recovery

When analyzing very large repositories in streaming mode, Codefang can save
checkpoints after each chunk. If the process crashes or is interrupted, the
analysis resumes from the last checkpoint instead of starting over.

### Checkpoint Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--checkpoint` | bool | true | Enable checkpointing |
| `--checkpoint-dir` | string | ~/.codefang/checkpoints | Directory for checkpoint files |
| `--resume` | bool | true | Resume from checkpoint if available |
| `--clear-checkpoint` | bool | false | Clear existing checkpoint before run |

### How It Works

When checkpointing is enabled (default) and streaming mode is active:
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
codefang history --streaming-mode=on -a burndown ~/sources/kubernetes

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

## Summary

The arena pattern is one piece of a broader design strategy:
reduce CGO overhead, keep memory contiguous, avoid GC churn, and reuse work
through caches and batch operations. The result is stable throughput on
large repositories without sacrificing correctness.

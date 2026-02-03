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

## Operational Guidance

- Increase `BlobCacheSize` when analyzing large repositories with high blob reuse.
- Increase `DiffCacheSize` when commits repeatedly touch the same files.
- Increase `BlobArenaSize` when large files are common and fallbacks are frequent.
- Reduce `Workers` if CPU contention outweighs parallel gains.

## Summary

The arena pattern is one piece of a broader design strategy:
reduce CGO overhead, keep memory contiguous, avoid GC churn, and reuse work
through caches and batch operations. The result is stable throughput on
large repositories without sacrificing correctness.

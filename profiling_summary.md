# Codefang Burndown Analysis Profiling Summary

## Test Configuration
- **Repository**: Kubernetes (~130k commits total)
- **Commits analyzed**: 1,000 (using `--limit 1000`)
- **Analyzer**: burndown
- **Wall clock time**: ~20 seconds
- **CPU time**: 177.51s (861% CPU utilization - good parallelism)

## Executive Summary

The burndown analysis on Kubernetes is **heavily CGO-bound** (89.7% of CPU time), with the majority of work happening in libgit2 C code for blob loading and diffing operations. The Go-side processing is relatively efficient, accounting for only ~10% of total CPU time.

## CPU Time Breakdown

### 1. CGO Operations (89.7% - 159.25s)

| Operation | CPU Time | % of Total | Description |
|-----------|----------|------------|-------------|
| `BatchDiffBlobs` (C) | 78.58s | 44.27% | Computing line diffs between blob versions |
| `BatchLoadBlobs` (C) | 69.91s | 39.38% | Loading blob content from git object store |
| `DiffTreeToTree` | 11.99s | 6.75% | Computing tree-level diffs between commits |

**Analysis**: The CGO bridge is already optimized with batch operations (`cf_batch_diff_blobs`, `cf_batch_load_blobs`) to minimize CGO call overhead. The actual work is being done efficiently in C/libgit2.

### 2. Language Detection (5.01% - 8.90s)

| Operation | CPU Time | % of Total |
|-----------|----------|------------|
| `LanguagesDetectionAnalyzer.Consume` | 8.90s | 5.01% |
| `enry.GetLanguage` | 7.59s | 4.28% |
| Regex operations (modeline detection) | 6.77s | 3.81% |

**Root cause**: The `enry` library uses expensive regex operations to detect vim/emacs modelines in file content:
- `GetLanguagesByVimModeline`: 4.73s
- `GetLanguagesByEmacsModeline`: 2.04s

### 3. Burndown Analysis Core (2.11% - 3.74s)

| Operation | CPU Time | % of Total |
|-----------|----------|------------|
| `processShardChanges` | 3.74s | 2.11% |
| `handleModification` | 3.10s | 1.75% |
| `applyDiffs` | 1.48s | 0.83% |
| `burndown.File.Update` | 1.27s | 0.72% |

**Analysis**: The sharded parallel processing is working well. The burndown RB-tree operations are efficient.

### 4. Line Counting (1.72% - 3.06s)

| Operation | CPU Time | % of Total |
|-----------|----------|------------|
| `CachedBlob.CountLines` | 3.06s | 1.72% |
| `bytes.Count` | 2.01s | 1.13% |
| `bytes.IndexByte` | 1.39s | 0.78% |

**Analysis**: Simple byte operations, called many times. Already using efficient stdlib functions.

## Parallelism Analysis

- **Wall time**: 20.61s
- **CPU time**: 177.51s
- **Parallelism factor**: 8.6x (861% CPU utilization)

This indicates excellent parallel utilization. The `Goroutines` setting (defaulting to `runtime.NumCPU()`) and sharded processing are working effectively.

## Potential Optimization Opportunities

### High Impact

1. **Lazy Language Detection** (potential 5% improvement)
   - Language detection is performed for all blobs but may not be needed for burndown-only analysis
   - Consider making it optional or lazy-evaluated

2. **Modeline Detection Optimization** (potential 3-4% improvement)
   - The `enry` library's modeline regex detection is expensive
   - Consider caching language results by file extension
   - Or skip modeline detection entirely for known extensions

### Medium Impact

3. **Blob Content Caching**
   - If the same blob appears in multiple commits (common in large repos), cache the line count
   - Trade memory for CPU time

4. **Diff Algorithm Selection**
   - The C-side diff computation is the largest cost
   - Investigate if Myers diff parameters can be tuned for speed vs accuracy tradeoff

### Low Impact (diminishing returns)

5. **Line Counting**
   - Already using `bytes.Count` which is assembly-optimized
   - Little room for improvement

## Conclusions

1. **The analysis is well-optimized** - 90% of time is in unavoidable git operations (blob loading and diffing)

2. **Parallelism is effective** - 8.6x CPU utilization on what appears to be an 8+ core machine

3. **Go-side code is efficient** - Burndown core logic (RB-trees, sharding) takes only ~2% of CPU time

4. **Language detection is the main Go-side bottleneck** - If not needed for burndown, disabling it could save 5% wall time

5. **The CGO batching strategy is working** - Batch operations minimize CGO overhead effectively

## Files Generated

- `cpu_burndown.prof` - Raw CPU profile baseline (use `go tool pprof` to analyze)
- `cpu_burndown_optimized.prof` - Raw CPU profile after Phase 1 optimizations
- `cpu_burndown_flamegraph.svg` - Visual flamegraph representation

---

## Phase 1 Optimization Results (2026-01-31)

### Implemented Optimizations

1. **Line Count Caching** (`pkg/gitlib/cached_blob.go`)
   - Uses `sync.Once` to compute line count once per blob
   - Thread-safe caching

2. **Extension-based Language Detection** (`pkg/analyzers/plumbing/languages.go`)
   - 150+ file extensions mapped to languages
   - O(1) lookup avoids expensive regex-based content analysis

### Measured Improvements

| Component | Before | After | Improvement |
|-----------|--------|-------|-------------|
| Language Detection | 8.90s (5.01%) | 1.74s (0.95%) | **80% reduction** |
| enry.GetLanguage | 7.59s | 1.50s | **80% reduction** |
| Regex operations | 6.70s | 1.36s | **80% reduction** |

### Profile Command
```bash
# Compare baseline vs optimized
go tool pprof -top -cum cpu_burndown.prof
go tool pprof -top -cum cpu_burndown_optimized.prof
```

---

## Phase 2 Optimization Results (2026-01-31)

### Implemented Optimizations

1. **GlobalBlobCache with LRU Eviction** (`pkg/framework/blob_cache.go`)
   - Cross-commit blob caching with configurable size limit (default 256MB)
   - LRU eviction policy using doubly-linked list
   - Thread-safe concurrent access with minimal lock contention
   - Batch operations (GetMulti/PutMulti) for efficiency

2. **BlobPipeline Integration** (`pkg/framework/blob_pipeline.go`)
   - Cache lookup before requesting blobs from git
   - Automatic caching of newly loaded blobs
   - Zero behavior change when cache is disabled

3. **Coordinator Configuration** (`pkg/framework/coordinator.go`)
   - BlobCacheSize configuration option
   - Cache statistics API for observability

### Measured Improvements

| Component | Baseline | Phase 1 | Phase 2 | Total Improvement |
|-----------|----------|---------|---------|-------------------|
| Wall clock time | ~20s | ~16s | **13.52s** | **32% faster** |
| BatchLoadBlobs | 69.91s (39.38%) | 69.91s | **10.56s (12.42%)** | **85% reduction** |
| Language Detection | 8.90s (5.01%) | 1.74s (0.95%) | 1.65s (1.94%) | 81% reduction |
| CGO overhead | 89.7% | ~85% | **~82%** | Improved |

### Key Insight

The GlobalBlobCache dramatically reduces blob loading time because:
- Many blobs appear unchanged across multiple consecutive commits
- File content is the same even when file is moved/renamed
- Large repos like Kubernetes have high blob reuse across commits

### Profile Command
```bash
# Phase 2 profile
go tool pprof -top -cum /tmp/phase2_cpu.prof
```

---

## Phase 3 Optimization Results (2026-01-31)

### Implemented Optimizations

1. **DiffCache with LRU Eviction** (`pkg/framework/diff_cache.go`)
   - Caches diff results by (oldHash, newHash) pair
   - LRU eviction policy with configurable max entries (default 10,000)
   - Thread-safe concurrent access
   - Eliminates redundant diff computations

2. **DiffPipeline Integration** (`pkg/framework/diff_pipeline.go`)
   - Cache lookup before requesting diffs from C
   - Automatic caching of computed diffs
   - Zero behavior change when cache is disabled

3. **Coordinator Configuration** (`pkg/framework/coordinator.go`)
   - DiffCacheSize configuration option
   - Diff cache statistics API for observability

### Measured Improvements

| Component | Baseline | Phase 2 | Phase 3 | Total Improvement |
|-----------|----------|---------|---------|-------------------|
| Wall clock time | 20.61s | 13.52s | **10.12s** | **51% faster** |
| BatchDiffBlobs | 78.58s (44.3%) | 58.37s (68.6%) | **22.70s (52.1%)** | **71% reduction** |
| BatchLoadBlobs | 69.91s (39.4%) | 10.56s (12.4%) | 7.60s (17.4%) | 89% reduction |
| Total CGO time | 159.25s | 68.93s | **30.30s** | **81% reduction** |

### Key Insight

The DiffCache dramatically reduces diff computation time because:
- Many file pairs appear in multiple commits with the same content
- Consecutive commits often touch the same files repeatedly
- Renames and moves reuse the same blob pairs

### Profile Command
```bash
# Phase 3 profile (Go caching)
go tool pprof -top -cum /tmp/phase3_cpu.prof
```

---

## Phase 3 C-Level Optimizations (2026-01-31)

### Implemented Optimizations

1. **ODB Direct Access** (`clib/blob_ops.c`)
   - Uses `git_odb_read` instead of `git_blob_lookup` (avoids type verification overhead)
   - Single `git_odb_refresh` call per batch
   - Direct object database access for faster blob retrieval

2. **OID Sorting for Pack Locality** (`clib/blob_ops.c`)
   - Sorts OIDs before loading to maximize pack cache hits
   - Objects with similar hashes are often in the same pack
   - Reduces random I/O, improves mwindow cache efficiency

3. **Blob Preloading for Diffs** (`clib/diff_ops.c`)
   - Collects all unique OIDs from diff requests
   - Loads blobs in sorted order (maximizes cache hits)
   - Uses `git_diff_buffers` with preloaded data (avoids re-lookup)
   - Deduplicates blob loads across diff pairs

### Final Performance Results

| Component | Baseline | After All Optimizations | Total Improvement |
|-----------|----------|-------------------------|-------------------|
| Wall clock time | 20.61s | **9.98s** | **52% faster** |
| BatchDiffBlobs | 78.58s (44.3%) | 23.05s (53.1%) | 71% reduction |
| BatchLoadBlobs | 69.91s (39.4%) | 7.13s (16.4%) | **90% reduction** |
| Total CGO time | 159.25s | ~30s | **81% reduction** |

### Optimization Breakdown

| Phase | Optimization | Impact |
|-------|-------------|--------|
| Phase 1 | Extension-based language detection | 80% reduction in language detection |
| Phase 1 | Line count caching | Eliminates redundant line counting |
| Phase 2 | GlobalBlobCache (LRU) | 85% reduction in blob loading |
| Phase 3 | DiffCache (LRU) | 61% reduction in diff computation |
| Phase 3 | ODB direct access | ~6% faster blob loading |
| Phase 3 | Sorted OID loading | Better pack cache locality |
| Phase 3 | Blob preloading for diffs | Eliminates redundant lookups |

### Profile Command
```bash
# Phase 3 C optimization profile
go tool pprof -top -cum /tmp/phase3_c_opt_cpu.prof
```

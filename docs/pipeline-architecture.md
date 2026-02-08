# Pipeline Architecture

This document describes the optimized 3-layer pipeline architecture used by Codefang for history analysis. The architecture is designed for high-performance processing of large git repositories with intelligent caching at multiple levels.

## Overview

The history analysis pipeline processes git commits through three distinct layers:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  LAYER 3: ANALYSIS                                                       │
│  pkg/analyzers/*                                                         │
│  Burndown, Devs, Couples, Sentiment, etc.                               │
│  Consumes pre-fetched data via Context                                  │
└─────────────────────────────┬───────────────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────────────┐
│  LAYER 2: PIPELINE ORCHESTRATION                                        │
│  pkg/framework/*                                                         │
│  Coordinator → CommitStreamer → BlobPipeline → DiffPipeline → Runner   │
│  + GlobalBlobCache (LRU) + DiffCache (LRU)                              │
└─────────────────────────────┬───────────────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────────────────┐
│  LAYER 1: DATA ACQUISITION                                              │
│  pkg/gitlib/*                                                            │
│  CGOBridge: BatchLoadBlobs, BatchDiffBlobs                              │
│  Workers: Thread-locked CGO workers for libgit2                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Layer 1: Data Acquisition (pkg/gitlib)

The data acquisition layer provides high-performance access to git objects through libgit2 via CGO.

### Key Components

#### CGOBridge
Provides batch operations to minimize CGO call overhead:
- `BatchLoadBlobs`: Load multiple blob contents in a single CGO call
- `BatchDiffBlobs`: Compute multiple diffs in a single CGO call

#### C-Level Optimizations (`clib/`)

**blob_ops.c - Pack-Aware Blob Loading:**
- Uses ODB API directly (`git_odb_read`) instead of `git_blob_lookup`
- Sorts OIDs before loading to maximize pack cache locality
- Single `git_odb_refresh` per batch for consistent state
- Falls back to unsorted loading for small batches (< 5 items)

**diff_ops.c - Optimized Diff Computation:**
- Preloads all unique blobs before computing diffs
- Deduplicates blob loads across diff pairs
- Uses `git_diff_buffers` with preloaded data (avoids re-lookup)
- Binary search for finding preloaded blobs

#### Worker Pool
- **Sequential Worker**: Handles tree diffs (must be sequential for correctness)
- **Pool Workers**: Handle blob loading and diff computation in parallel
- Each worker is locked to an OS thread (required by libgit2)
- Each pool worker has its own repository handle to avoid contention

#### CachedBlob
Wrapper around blob data with optimizations:
- **Line count caching**: Uses `sync.Once` to compute line count only once
- **Binary detection**: Cached result of binary content check

```go
type CachedBlob struct {
    hash          Hash
    Data          []byte
    lineCount     int
    lineCountOnce sync.Once
}
```

### Language Detection Optimization

The `LanguagesDetectionAnalyzer` uses a fast-path for common file extensions:
- 150+ file extensions mapped to languages for O(1) lookup
- Falls back to `enry` library only when extension is unknown
- Reduces language detection overhead by ~80%

## Layer 2: Pipeline Orchestration (pkg/framework)

The orchestration layer chains processing stages and manages caching.

### Coordinator

The central orchestrator that:
1. Creates and manages worker pools
2. Chains pipeline stages
3. Configures caching layers

```go
type CoordinatorConfig struct {
    BatchConfig     gitlib.BatchConfig
    CommitBatchSize int
    Workers         int
    BufferSize      int
    BlobCacheSize   int64  // Default: 256MB
    DiffCacheSize   int    // Default: 10,000 entries
}
```

### Pipeline Stages

#### 1. CommitStreamer
Batches commits for efficient processing:
```
Commits[] → CommitBatch{Commits, StartIndex}
```

#### 2. BlobPipeline
Loads blob content with caching:
```
CommitBatch → BlobData{Commit, Changes, BlobCache}
```

**Optimizations:**
- Checks GlobalBlobCache before requesting from git
- Stores newly loaded blobs in cache
- Batch requests to minimize CGO calls

#### 3. DiffPipeline
Computes file diffs with caching:
```
BlobData → CommitData{Commit, Changes, BlobCache, FileDiffs}
```

**Optimizations:**
- Checks DiffCache before computing
- Stores computed diffs in cache
- Falls back to Go-based diff if C diff fails

### Caching Architecture

#### GlobalBlobCache

LRU cache for blob content, keyed by blob hash.

```go
type GlobalBlobCache struct {
    entries     map[Hash]*cacheEntry
    head, tail  *cacheEntry  // LRU doubly-linked list
    maxSize     int64        // Bytes limit
    currentSize int64
}
```

**Why it works:**
- Many blobs appear unchanged across consecutive commits
- File content is identical even when files are moved/renamed
- Large repositories have very high blob reuse

**Performance impact:** 85% reduction in blob loading time

#### DiffCache

LRU cache for diff results, keyed by (oldHash, newHash) pair.

```go
type DiffCache struct {
    entries    map[DiffKey]*diffCacheEntry
    head, tail *diffCacheEntry  // LRU doubly-linked list
    maxEntries int
}

type DiffKey struct {
    OldHash Hash
    NewHash Hash
}
```

**Why it works:**
- Many file pairs appear in multiple commits with same content
- Consecutive commits often touch the same files repeatedly
- File renames/moves reuse the same blob pairs

**Performance impact:** 71% reduction in diff computation time

### Data Flow Diagram

```
                    ┌──────────────────┐
                    │     Commits      │
                    └────────┬─────────┘
                             │
                    ┌────────▼─────────┐
                    │  CommitStreamer  │
                    │   (batching)     │
                    └────────┬─────────┘
                             │
        ┌────────────────────▼────────────────────┐
        │              BlobPipeline               │
        │  ┌─────────────────────────────────┐    │
        │  │       GlobalBlobCache           │    │
        │  │  ┌─────┐  Hit?  ┌──────────┐   │    │
        │  │  │Check├───Yes──►Return    │   │    │
        │  │  └──┬──┘        └──────────┘   │    │
        │  │     │No                        │    │
        │  │  ┌──▼──────────┐               │    │
        │  │  │BatchLoadBlob│               │    │
        │  │  └──┬──────────┘               │    │
        │  │     │                          │    │
        │  │  ┌──▼──┐                       │    │
        │  │  │Store│                       │    │
        │  │  └─────┘                       │    │
        │  └─────────────────────────────────┘    │
        └────────────────────┬────────────────────┘
                             │
        ┌────────────────────▼────────────────────┐
        │              DiffPipeline               │
        │  ┌─────────────────────────────────┐    │
        │  │          DiffCache              │    │
        │  │  ┌─────┐  Hit?  ┌──────────┐   │    │
        │  │  │Check├───Yes──►Return    │   │    │
        │  │  └──┬──┘        └──────────┘   │    │
        │  │     │No                        │    │
        │  │  ┌──▼──────────┐               │    │
        │  │  │BatchDiffBlob│               │    │
        │  │  └──┬──────────┘               │    │
        │  │     │                          │    │
        │  │  ┌──▼──┐                       │    │
        │  │  │Store│                       │    │
        │  │  └─────┘                       │    │
        │  └─────────────────────────────────┘    │
        └────────────────────┬────────────────────┘
                             │
                    ┌────────▼─────────┐
                    │      Runner      │
                    │  (feed analyzers)│
                    └────────┬─────────┘
                             │
                    ┌────────▼─────────┐
                    │    Analyzers     │
                    └──────────────────┘
```

## Layer 3: Analysis (pkg/analyzers)

Analyzers consume pre-fetched data through a standardized context.

### Context Structure

```go
type Context struct {
    Commit    *gitlib.Commit
    Index     int
    Time      time.Time
    IsMerge   bool
    Changes   gitlib.Changes
    BlobCache map[gitlib.Hash]*gitlib.CachedBlob
    FileDiffs map[string]plumbing.FileDiffData
}
```

### Analyzer Interface

```go
type HistoryAnalyzer interface {
    Name() string
    Configure(facts map[string]any) error
    Initialize(repo *gitlib.Repository) error
    Consume(ctx *Context) error
    Finalize() (Report, error)
}
```

### Core Analyzers

| Analyzer | Purpose | Key Data Used |
|----------|---------|---------------|
| Burndown | Track code age and churn | FileDiffs, BlobCache |
| Devs | Developer contribution metrics | Changes, LineStats |
| Couples | File coupling analysis | Changes |
| Sentiment | Code sentiment over time | UASTChanges |
| FileHistory | Per-file history tracking | Changes, LineStats |

### Plumbing Analyzers

Support analyzers that pre-compute shared data:

- **TreeDiffAnalyzer**: Computes tree-level diffs
- **BlobCacheAnalyzer**: Manages blob data access
- **FileDiffAnalyzer**: Computes file-level diffs
- **LanguagesDetectionAnalyzer**: Detects file languages (with extension fast-path)
- **LinesStatsCalculator**: Computes line statistics
- **UASTChangesAnalyzer**: Computes AST-level changes

## Performance Characteristics

### Benchmarks (Kubernetes repo, 1000 commits)

| Phase | Wall Time | Improvement |
|-------|-----------|-------------|
| Baseline | 20.61s | - |
| + Line count caching | ~19s | 8% |
| + Language detection fast-path | ~16s | 22% |
| + GlobalBlobCache | 13.52s | 34% |
| + DiffCache | 10.12s | 51% |
| + C-level optimizations | 9.98s | **52%** |

### CPU Time Breakdown (After All Optimizations)

| Component | CPU Time | Percentage |
|-----------|----------|------------|
| BatchDiffBlobs (C) | 23.05s | 53% |
| BatchLoadBlobs (C) | 7.13s | 16% |
| Burndown processing | 1.84s | 4% |
| Language detection | ~1s | 2% |
| Other | ~11s | 25% |

### Memory Usage

- **GlobalBlobCache**: Up to 256MB (configurable)
- **DiffCache**: Up to 10,000 entries (configurable)
- **Worker repos**: ~50MB per worker

## Configuration

### Default Configuration

```go
CoordinatorConfig{
    CommitBatchSize: 1,
    Workers:         runtime.NumCPU(),
    BufferSize:      Workers * 2,
    BlobCacheSize:   256 * 1024 * 1024,  // 256MB
    DiffCacheSize:   10000,               // entries
}
```

### Tuning Guidelines

| Scenario | Recommendation |
|----------|----------------|
| Memory constrained | Reduce BlobCacheSize |
| Large files | Increase BlobCacheSize |
| Many small commits | Increase DiffCacheSize |
| Few CPUs | Reduce Workers |

## Thread Safety

All caches are thread-safe:
- Use `sync.RWMutex` for concurrent read access
- Use `sync.Once` for one-time computations
- Atomic counters for statistics

Worker pools are designed for safe concurrent access:
- Each worker locked to OS thread
- Separate repository handles per worker
- Channel-based request/response communication

## Extending the Architecture

### Adding a New Cache

1. Implement LRU cache with same pattern as `GlobalBlobCache`
2. Add to Coordinator configuration
3. Integrate with appropriate pipeline stage
4. Add statistics method to Coordinator

### Adding a New Analyzer

1. Implement `HistoryAnalyzer` interface
2. Register in `analyzerPipeline` (cmd/codefang/commands/history.go)
3. Declare dependencies on plumbing analyzers
4. Access pre-computed data through Context

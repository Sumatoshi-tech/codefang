# Reusable Code Specification

This document organizes reusable code patterns found in the codebase into logical clusters around problem domains. Each cluster represents a cohesive area of functionality that could be extracted into shared packages.

---

## Cluster 1: Concurrency & Parallel Processing

**Problem Domain**: Safe concurrent execution, worker management, and parallel processing patterns.

### 1.1 Worker Pool Pattern
**Location**: `pkg/pipeline/workerpool.go`
**Components**:
- `WorkerPool[T]` - Generic bounded concurrency worker pool
- `Run()` - Process slices with bounded parallelism
- `RunChan()` - Process channels with bounded parallelism
- Error handling with `sync.Once` (first error wins)
- Context cancellation propagation
- Auto-scaling workers to `min(MaxParallel, itemCount)`

**Reuse Potential**: HIGH - Replace ad-hoc goroutine loops throughout codebase

### 1.2 Producer-Consumer Pattern
**Location**: `pkg/pipeline/runpc.go`
**Components**:
- `RunPC[In, Out, Job]` - Owned goroutine topology
- Channel creation, goroutine spawning, orderly shutdown
- Configurable buffer size

**Reuse Potential**: HIGH - Standard producer-consumer template

### 1.3 Channel Coordination
**Location**: `pkg/pipeline/drain.go`
**Components**:
- `SignalOnDrain()` - Forward items while signaling completion
- Independent pipeline stage span ending

**Reuse Potential**: MEDIUM - Channel-based pipeline coordination

### 1.4 Thread-Restricted CGO Worker
**Location**: `pkg/gitlib/worker.go`
**Components**:
- `Worker` - Single-threaded CGO worker
- `runtime.LockOSThread()` for libgit2 safety
- Request handling via channel
- `WithContext` decorator for context injection

**Reuse Potential**: MEDIUM - Pattern for thread-restricted CGO libraries

### 1.5 Signal Handling & Graceful Shutdown
**Location**: `pkg/sigutil/sigutil.go`
**Components**:
- `SignalCleanupGuard` - Ensures cleanup runs exactly once
- Handles SIGINT/SIGTERM
- Uses `sync.Once` for idempotency
- Logger integration

**Reuse Potential**: HIGH - Superior to ad-hoc signal handling

---

## Cluster 2: Memory Optimization

**Problem Domain**: Reducing GC pressure, efficient memory management, and pooling strategies.

### 2.1 Memory Pooling (sync.Pool)
**Location**: `pkg/uast/pkg/node/node.go`
**Components**:
- `posPool` - Positions struct pooling
- `nodePool` - Node struct pooling
- `Release()` - Return to pool
- `ReleaseTree()` - Iterative tree cleanup

**Reuse Potential**: HIGH - Applicable to any hot path with frequent allocations

### 2.2 Per-Worker Free-List Allocator
**Location**: `pkg/uast/pkg/node/allocator.go`
**Components**:
- `Allocator` - Per-worker free-list
- `GetNode()`/`PutNode()` - Node allocation
- `GetPositions()`/`PutPositions()` - Position allocation
- Avoids `sync.Pool` cross-goroutine contention

**Reuse Potential**: HIGH - Superior to sync.Pool for single-threaded scenarios

### 2.3 Memory Monitoring
**Location**: `pkg/meminfo/meminfo.go`
**Components**:
- `ReadRSSBytes()` - Process RSS memory reading
- Platform-specific (Linux via /proc/self/statm)

**Reuse Potential**: LOW - Platform-specific utility

---

## Cluster 3: Data Structures & Algorithms

**Problem Domain**: Efficient data structures, algorithms, and mathematical operations.

### 3.1 Bloom Filter
**Location**: `pkg/alg/bloom/bloom.go`
**Components**:
- `Filter` - Thread-safe Bloom filter
- Double-hashing technique
- Add, Test, TestAndAdd operations
- Bulk operations, serialization, estimation

**Reuse Potential**: HIGH - Already complete and generic

**Dedup Opportunity**: Replace `pkg/uast/loader.go` bloom functions (bloomAdd, bloomMayContain, bloomHashes)

### 3.2 Tree Traversal
**Location**: `pkg/alg/tree.go`
**Components**:
- `TraverseTree` - Iterative pre-order DFS
- Already used by `internal/analyzers/common/uast_traversal.go`

**Reuse Potential**: MEDIUM - Already being reused

**Dedup Opportunity**: Replace `pkg/uast/pkg/node/node.go` VisitPreOrder (similar functionality)

### 3.3 Generic Stack
**Location**: `internal/analyzers/common/context_stack.go`
**Components**:
- `ContextStack[T]` - Generic LIFO stack
- Push/Pop/Current/Depth methods
- Type-safe with generics

**Reuse Potential**: HIGH - Replace ad-hoc slice-based stacks

### 3.4 Chunking / Batching
**Location**: `pkg/alg/chunk.go`
**Components**:
- `Chunk` - Split range [0, total) into chunks
- Returns []Range with Start/End

**Reuse Potential**: MEDIUM - Batch processing utility

**Dedup Opportunity**: Replace inline chunk loops in:
- `scripts/bench-hibernation/main.go:291`
- `internal/framework/commit_streamer.go:49`
- `internal/analyzers/common/renderer/renderer.go:148`

### 3.5 Classification
**Location**: `internal/analyzers/common/classify.go`
**Components**:
- `Classifier[T]` - Generic threshold-based classifier
- Works with any `cmp.Ordered` type
- Sorts thresholds descending
- `Classify()` returns first matching label

**Reuse Potential**: HIGH - More general than `ThresholdLabeler`

**Dedup Opportunity**: Replace `internal/analyzers/common/threshold_labeler.go` (redundant)

### 3.6 Statistical Functions
**Location**: `pkg/alg/stats/stats.go`
**Components**:
- Mean, StdDev, Percentile, ZScore
- Already used by multiple analyzers

**Reuse Potential**: HIGH - Already in use

### 3.7 Other Algorithm Packages (Already In Use)
- `pkg/alg/cms/cms.go` - Count-Min Sketch
- `pkg/alg/hll/hll.go` - HyperLogLog
- `pkg/alg/interval/interval.go` - Interval tree
- `pkg/alg/levenshtein/levenshtein.go` - Edit distance
- `pkg/alg/lru/cache.go` - LRU cache
- `pkg/alg/lsh/lsh.go` - LSH index
- `pkg/alg/minhash/minhash.go` - MinHash signatures
- `pkg/alg/mapx/` - Map and slice utilities
- `pkg/alg/pairs.go` - Pair iteration

---

## Cluster 4: Type Safety & Conversion

**Problem Domain**: Safe type conversions, overflow checking, and type extraction.

### 4.1 Generic Safe Conversions
**Location**: `pkg/safeconv/generic.go`
**Components**:
- `MustConvert[From, To]` - Overflow-checking (panics)
- `SafeConvert[From, To]` - Clamping on overflow
- `Extract[T]` - Type assertion with numeric coercion fallback
- `numericCoerce` - Reflect-based numeric conversion
- `maxVal`/`minVal` - Type-safe bounds computation

**Reuse Potential**: HIGH - Superior to ad-hoc type assertions

### 4.2 Convenience Wrappers
**Location**: `pkg/safeconv/safeconv.go`
**Components**:
- `ToInt`, `ToFloat64` - Extract from any
- `MustUintToInt`, `MustIntToUint`, etc. - Legacy wrappers

**Reuse Potential**: MEDIUM - Replace ad-hoc type assertions

**Dedup Opportunity**: Replace `val.(int)` patterns throughout codebase

---

## Cluster 5: File & IO Operations

**Problem Domain**: Safe file handling, path validation, and file filtering.

### 5.1 Safe File Reading
**Location**: `pkg/iosafety/iosafety.go`
**Components**:
- `ReadFile` - Safe file reading with path validation
- `ResolvePath` - Path normalization + validation
  - Empty check, NUL byte check
  - Directory check, stat check
- `SanitizeForTerminal` - Control character stripping

**Reuse Potential**: HIGH - Replace direct `os.ReadFile` calls

**Dedup Opportunity**: Replace ad-hoc path validation logic

### 5.2 File Filtering
**Location**: `pkg/pathfilter/pathfilter.go`
**Components**:
- `Filter` - Immutable file filter
- Combines enry vendor detection + custom rules
- `IsExcluded`, `IsExcludedWithContent`
- Generated file detection via markers
- Option pattern for configuration

**Reuse Potential**: HIGH - Well-designed filter pattern

### 5.3 Text Utilities
**Location**: `pkg/textutil/textutil.go`
**Components**:
- `IsBinary` - Null-byte sniffing (first 8000 bytes)
- `CountLines` - Proper trailing newline handling
- `WriteJSON` - JSON encoding with pretty-print option

**Reuse Potential**: MEDIUM - Text processing utilities

### 5.4 Persistence
**Location**: `pkg/persist/persist.go`
**Components**:
- `Codec` interface - Pluggable serialization
- `JSONCodec`, `GobCodec` - Implementations
- `SaveState`, `LoadState` - File persistence helpers
- `Persister[T]` - Generic type-safe wrapper

**Reuse Potential**: HIGH - Serialization abstraction pattern

---

## Cluster 6: Time & Units

**Problem Domain**: Time manipulation, bucketing, and unit conversions.

### 6.1 Time Bucketing
**Location**: `pkg/timeutil/timeutil.go`
**Components**:
- `FloorTime` - Round timestamp down to nearest tick boundary
- Handles `Round` edge case (after t)

**Reuse Potential**: MEDIUM - Replace ad-hoc time rounding

### 6.2 Multi-Format Time Parsing
**Location**: `pkg/gitlib/helpers.go`
**Components**:
- `ParseTime` - Duration, RFC3339, date-only formats
- `ResolveTime` - SHA/ref to timestamp resolution

**Reuse Potential**: MEDIUM - Flexible time parsing

### 6.3 Binary Size Units
**Location**: `pkg/units/units.go`
**Components**:
- `KiB`, `MiB`, `GiB` - 1024-based multipliers

**Reuse Potential**: LOW - Simple constants

**Dedup Opportunity**: Replace hardcoded `1024`, `1024*1024` throughout codebase

---

## Cluster 7: Tree & Graph Operations

**Problem Domain**: Tree traversal, transformation, navigation, and ID generation.

### 7.1 Tree Traversal
**Location**: `pkg/uast/pkg/node/node.go`
**Components**:
- `VisitPreOrder` - Iterative pre-order (stack-based)
- `VisitPostOrder` - Iterative post-order
- `PreOrder()` - Channel-based streaming
- Depth-limited to prevent stack overflow

**Reuse Potential**: HIGH - Replace recursive traversals

### 7.2 Tree Transformation
**Location**: `pkg/uast/pkg/node/node.go`
**Components**:
- `Transform` - Functional transformation (new tree)
- `TransformInPlace` - Mutation
- Post-order application

**Reuse Potential**: HIGH - General tree transformation pattern

### 7.3 Tree Search
**Location**: `pkg/uast/pkg/node/node.go`
**Components**:
- `Find` - Predicate-based pre-order search
- Returns matching nodes

**Reuse Potential**: MEDIUM - Generic tree search

### 7.4 Tree Navigation
**Location**: `pkg/uast/pkg/node/node.go`
**Components**:
- `Ancestors` - Iterative ancestor path finding
- Returns path from root to parent

**Reuse Potential**: MEDIUM - Tree navigation utility

### 7.5 Stable ID Generation
**Location**: `pkg/uast/pkg/node/node.go`
**Components**:
- `AssignStableIDs` - Content-based SHA1 hashing
- Deterministic ID generation
- Processes children first

**Reuse Potential**: MEDIUM - Stable ID for tree/graph structures

### 7.6 UAST Traversal Wrapper
**Location**: `internal/analyzers/common/uast_traversal.go`
**Components**:
- `UASTTraverser` - Wrapper around `pkg/alg/TraverseTree`
- `FindNodes`, `FindNodesByType`, `FindNodesByRoles`
- `CountLines` - Recursive line counting

**Reuse Potential**: LOW - Already uses pkg/alg

---

## Cluster 8: Builder & Construction Patterns

**Problem Domain**: Object construction, fluent APIs, and complex object creation.

### 8.1 Builder Pattern
**Location**: `pkg/uast/pkg/node/node.go`
**Components**:
- `Builder` - Fluent interface for Node construction
- `WithID`, `WithType`, `WithToken`, etc. - Chainable setters
- `Build()` - Final object creation
- `New()` - One-liner construction

**Reuse Potential**: HIGH - Template for other complex structs

### 8.2 Result Building
**Location**: `internal/analyzers/common/result_builder.go`
**Components**:
- `ResultBuilder` - Build analyze.Report maps
- `BuildEmptyResult`, `BuildBasicResult`, `BuildDetailedResult`
- `BuildCollectionResult`, `BuildMetricResult`

**Reuse Potential**: MEDIUM - Report building pattern

---

## Cluster 9: Pipeline & Stage Processing

**Problem Domain**: Pipeline construction, stage chaining, and batch processing.

### 9.1 Batch Accumulation
**Location**: `pkg/pipeline/batcher.go`
**Components**:
- `Batcher[In, Batch]` interface - Add/Flush
- `ThresholdBatcher` - Batch after N items
- `PassthroughBatcher` - 1 item = 1 batch

**Reuse Potential**: MEDIUM - Stream processing batching

### 9.2 Chain of Responsibility
**Location**: `pkg/pipeline/phase.go`
**Components**:
- `Phase[S]` interface - Single processing stage
- `PhaseFunc` adapter
- `RunPhases` - Sequential execution, state threading

**Reuse Potential**: HIGH - Pipeline stage pattern

### 9.3 Cache Decorator Pattern
**Location**: `pkg/pipeline/fetcher.go`
**Components**:
- `Fetcher[Req, Resp]` interface - Request-response
- `FetcherFunc` adapter
- Base for cache decoration

**Reuse Potential**: MEDIUM - Pluggable data fetching

### 9.4 Shared Response Caching
**Location**: `pkg/pipeline/shared_response.go`
**Components**:
- `SharedResponse[T]` - sync.Once caching with context
- Compute function evaluated exactly once
- Thread-safe result caching

**Reuse Potential**: HIGH - Lazy initialization + caching

### 9.5 Configuration Options
**Location**: `pkg/pipeline/options.go`
**Components**:
- `ConfigurationOption` - Unified config type
- Supports bool/int/string/float/[]string/path
- `FormatDefault()` for CLI display

**Reuse Potential**: MEDIUM - Configuration system pattern

---

## Cluster 10: Parsing & DSL

**Problem Domain**: DSL parsing, pattern matching, and grammar analysis.

### 10.1 Pattern Matcher with Caching
**Location**: `pkg/uast/pkg/mapping/pattern_matcher.go`
**Components**:
- `PatternMatcher` - Tree-sitter query caching
- `sync.Pool` for query cursor reuse
- Thread-safe with RWMutex
- Cache stats tracking (hits/misses)

**Reuse Potential**: HIGH - Cached query executor pattern

### 10.2 DSL Parsing Utilities
**Location**: `pkg/uast/pkg/mapping/dsl_parser.go`
**Components**:
- `parseQuotedList` - Comma-separated quoted strings
- `quotedListParser` - State machine
- `extractText`, `findChild` - AST traversal helpers
- `buildRulesFromAST` - AST-to-struct pattern

**Reuse Potential**: MEDIUM - DSL/config parsing

### 10.3 Identifier Validation
**Location**: `pkg/uast/pkg/mapping/grammar_analysis.go`
**Components**:
- `isValidIdentifier` - Go-style identifier validation
- Letter/underscore start, alphanumeric continuation

**Reuse Potential**: LOW - Simple validation utility

### 10.4 Coverage Analysis
**Location**: `pkg/uast/pkg/mapping/grammar_analysis.go`
**Components**:
- `CoverageAnalysis` - Mapped/total ratio
- Coverage statistics calculation

**Reuse Potential**: LOW - Mapping coverage utility

---

## Cluster 11: Metrics & Analysis

**Problem Domain**: Metric computation, aggregation, risk assessment, and reporting.

### 11.1 Metric Definition
**Location**: `pkg/metrics/metrics.go`
**Components**:
- `Metric[In, Out]` interface - Self-contained computation
- `Name`, `DisplayName`, `Description`, `Type`, `Compute`
- `MetricMeta` - Embeddable metadata struct
- `Registry` - Metric collection

**Reuse Potential**: HIGH - Metric definition pattern

### 11.2 Risk Assessment
**Location**: `pkg/metrics/metrics.go`
**Components**:
- `RiskLevel` - CRITICAL/HIGH/MEDIUM/LOW
- `RiskPriority` - Sortable integer mapping
- `RiskResult` - Output structure

**Reuse Potential**: MEDIUM - Risk sorting utility

### 11.3 Metric Aggregation
**Location**: `internal/analyzers/common/metrics_processor.go`
**Components**:
- `MetricsProcessor` - Extract and aggregate metrics
- `ProcessReport`, `CalculateAverages`, `GetCounts`
- Uses `pkg/safeconv` for type conversion

**Reuse Potential**: MEDIUM - Already uses pkg/safeconv

### 11.4 Report Aggregation
**Location**: `internal/analyzers/common/aggregator.go`
**Components**:
- `Aggregator` - Combines MetricsProcessor + SpillableDataCollector + ResultBuilder
- Thread-safe with sync.Mutex
- `Aggregate` multiple reports

**Reuse Potential**: LOW - Composition of existing components

### 11.5 Spillable Data Collection
**Location**: `internal/analyzers/common/spillable_data_collector.go`
**Components**:
- Transparent spill-to-disk when buffer exceeds threshold
- Gob encoding
- Composite key support
- Aggregation mode (SummaryOnly vs Full)

**Reuse Potential**: HIGH - Memory-efficient data collection

### 11.6 Analyzer Framework
**Location**: `internal/analyzers/analyze/analyzer.go`
**Components**:
- `Analyzer` interface - Base contract
- `StaticAnalyzer` interface - UAST-based analysis
- `VisitorProvider` - Single-pass optimization
- `Factory` - Plugin registry and execution
- `RunAnalyzers` - Parallel (uses WorkerPool) and sequential modes

**Reuse Potential**: HIGH - Plugin/extension pattern

### 11.7 Analyzer Registry
**Location**: `internal/analyzers/analyze/registry.go`
**Components**:
- `Registry` - Analyzer metadata with deterministic ordering
- Glob pattern support (*, ?, [])
- Split by mode
- Duplicate detection

**Reuse Potential**: HIGH - Plugin registry with pattern matching

### 11.8 Typed Collection
**Location**: `internal/analyzers/analyze/typed_collection.go`
**Components**:
- `TypedCollection` - Deferred map conversion
- Avoids premature map allocation
- `ItemConverter` - Conversion function

**Reuse Potential**: MEDIUM - Lazy conversion pattern

---

## Cluster 12: Terminal UI & Formatting

**Problem Domain**: Terminal rendering, progress bars, tables, and text formatting.

### 12.1 Progress Bars
**Location**: `internal/analyzers/common/terminal/progress.go`
**Components**:
- `DrawProgressBar` - █/░ characters, clamps [0,1]
- `FormatScore` - "N/10" format
- `FormatScoreBar` - "[████████░░] 8/10"
- `DrawPercentBar` - Labeled percentage with count

**Reuse Potential**: MEDIUM - Progress visualization

### 12.2 Box Drawing
**Location**: `internal/analyzers/common/terminal/box.go`
**Components**:
- `DrawSeparator` - Thin horizontal line
- `DrawHeader` - Heavy-bordered section header
- Box characters (light, heavy, rounded)

**Reuse Potential**: LOW - Terminal UI utilities

### 12.3 Report Formatting
**Location**: `internal/analyzers/common/formatter.go`
**Components**:
- `Formatter` - Analysis report formatter
- `createProgressBar` - Progress bar with status emoji
- `formatCollectionTable` - go-pretty table rendering
- `extractAllNumericMetrics` - Metric extraction
- Sorting, limiting, key extraction

**Reuse Potential**: MEDIUM - Report formatting pattern

---

## Cluster 13: Version & Build Info

**Problem Domain**: Build version extraction and binary metadata.

### 13.1 Version Extraction
**Location**: `pkg/version/version.go`
**Components**:
- `InitBinaryVersion` - Extract API version from package path
- Uses reflection on `reflect.TypeFor`
- Sets `Binary` variable

**Reuse Potential**: LOW - Version extraction pattern

---

## Summary: Priority Actions

### Immediate Dedup Opportunities (High ROI)
1. **Replace pkg/uast/loader.go bloom functions** with `pkg/alg/bloom/bloom.go`
2. **Replace ad-hoc type assertions** with `pkg/safeconv` generics
3. **Replace ad-hoc file reading** with `pkg/iosafety`
4. **Replace inline chunk loops** with `pkg/alg/chunk.go`
5. **Replace ThresholdLabeler** with `Classifier[float64]`
6. **Replace hardcoded 1024 constants** with `pkg/units`

### High-Value Reusable Patterns (Extract to Shared Packages)
1. **WorkerPool** (pkg/pipeline) - Already well-designed
2. **Classifier** (internal/analyzers/common) - Extract to pkg/alg or pkg/classify
3. **ContextStack** (internal/analyzers/common) - Extract to pkg/alg or pkg/stack
4. **SharedResponse** (pkg/pipeline) - Already well-designed
5. **TypedCollection** (internal/analyzers/analyze) - Pattern for lazy conversion
6. **SpillableDataCollector** (internal/analyzers/common) - Memory-efficient collection

### Already Well-Designed (No Action Needed)
- All `pkg/alg/*` packages (already in use)
- `pkg/pipeline/*` packages
- `pkg/metrics/metrics.go`
- `pkg/sigutil/sigutil.go`
- `pkg/pathfilter/pathfilter.go`
- `pkg/persist/persist.go`

---

## Recommended Package Structure

```
pkg/
  alg/          # Algorithms (complete)
  classify/     # NEW: Extract Classifier from internal/analyzers/common
  concurrent/   # NEW: WorkerPool, RunPC, SharedResponse (from pkg/pipeline)
  iosafety/     # Already exists (complete)
  metrics/      # Already exists (complete)
  pathfilter/   # Already exists (complete)
  persist/      # Already exists (complete)
  pipeline/     # Keep Phase, Batcher, Fetcher interfaces
  safeconv/     # Already exists (complete)
  sigutil/      # Already exists (complete)
  stack/        # NEW: Extract ContextStack from internal/analyzers/common
  textutil/     # Already exists (complete)
  timeutil/     # Already exists (complete)
  units/        # Already exists (complete)
  uast/         # Keep UAST-specific code
```

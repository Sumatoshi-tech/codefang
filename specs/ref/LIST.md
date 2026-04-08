# Reusable Code Inventory

## Dedup opportunities

1. pkg/alg/bloom/bloom.go

   Function: Filter (type)
   Position: pkg/alg/bloom/bloom.go:33-260
   Findings: Complete, thread-safe Bloom filter implementation with double-hashing technique. Supports Add, Test, TestAndAdd, bulk operations, serialization, and estimation. Already generic (works with []byte).
   Could replace:
     - pkg/uast/loader.go:174:189:bloomAdd (simple bloom filter for extension lookup)
     - pkg/uast/loader.go:191:197:bloomMayContain
     - pkg/uast/loader.go:200:213:bloomHashes

2. pkg/alg/chunk.go

   Function: Chunk
   Position: pkg/alg/chunk.go:10-22
   Findings: Generic utility to split a range [0, total) into chunks of given size. Returns []Range with Start/End. Simple, reusable for batch processing and parallelization.
   Could replace:
     - scripts/bench-hibernation/main.go:291:291 (inline chunk loop)
     - internal/framework/commit_streamer.go:49:49 (inline batch loop: i += s.BatchSize)
     - internal/analyzers/common/renderer/renderer.go:148:148 (inline chunk loop: i += MetricsPerRow)

3. pkg/alg/tree.go

   Function: TraverseTree
   Position: pkg/alg/tree.go:8-29
   Findings: Generic iterative pre-order DFS tree traversal. Already being used in internal/analyzers/common/uast_traversal.go. Could potentially replace more complex custom traversals where simple pre-order is sufficient.
   Already used by:
     - internal/analyzers/common/uast_traversal.go:48

4. pkg/uast/pkg/mapping/pattern_matcher.go

   Function: PatternMatcher (type)
   Position: pkg/uast/pkg/mapping/pattern_matcher.go:17-104
   Findings: Pattern matcher with LRU-style caching for tree-sitter queries. Uses sync.Pool for query cursor reuse. Thread-safe with RWMutex. Cache stats tracking (hits/misses).
   Reusability: The caching + sync.Pool pattern could be extracted as a generic "cached query executor" component.

5. pkg/uast/pkg/mapping/grammar_analysis.go

   Function: isValidIdentifier
   Position: pkg/uast/pkg/mapping/grammar_analysis.go:71-86
   Findings: Validates Go-style identifier names (letter/underscore start, alphanumeric continuation). Simple, reusable validation utility.
   Could replace:
     - Any ad-hoc identifier validation in code generators or parsers

   Function: parseQuotedList
   Position: pkg/uast/pkg/mapping/dsl_parser.go:127-147
   Findings: Parses comma-separated quoted strings with state machine. Reusable for DSL/config parsing.
   
   Function: CoverageAnalysis
   Position: pkg/uast/pkg/mapping/grammar_analysis.go:52-68
   Findings: Coverage statistics calculation (mapped/total ratio). Reusable for any mapping/coverage analysis.

6. pkg/uast/types.go

   Function: getFileExtension
   Position: pkg/uast/types.go:237-243
   Findings: Returns file extension with dot prefix.
   Could replace:
     - Standard filepath.Ext could be used instead (dedup opportunity d)

7. pkg/iosafety/iosafety.go

   Function: ReadFile
   Position: pkg/iosafety/iosafety.go:26-39
   Findings: Safe file reading with path validation. Returns content + resolved absolute path.
   Could replace:
     - Any direct os.ReadFile calls that lack path validation

   Function: ResolvePath
   Position: pkg/iosafety/iosafety.go:42-68
   Findings: Path normalization + validation (empty check, NUL byte check, directory check, stat check).
   Could replace:
     - Ad-hoc path validation logic throughout codebase

   Function: SanitizeForTerminal
   Position: pkg/iosafety/iosafety.go:71-83
   Findings: Strips control characters, HTML-escapes output. Useful for logging user-provided strings safely.
   Could replace:
     - Ad-hoc string sanitization in logging/output code

8. pkg/uast/pkg/node/node.go

   Function: sync.Pool usage (posPool, nodePool)
   Position: pkg/uast/pkg/node/node.go:113-118, 166-170
   Findings: Memory pooling pattern for frequently allocated structs (Positions, Node). Reduces GC pressure.
   Reusability: General memory pooling pattern applicable to any hot path with frequent allocations.

   Function: Builder pattern
   Position: pkg/uast/pkg/node/node.go:175-223
   Findings: Fluent builder for Node construction. Clean API for complex object creation.
   Reusability: Builder pattern template for other complex structs.

   Function: VisitPreOrder / VisitPostOrder
   Position: pkg/uast/pkg/node/node.go:302-322, 325-328
   Findings: Iterative tree traversal without recursion. Stack-based, depth-limited.
   Could replace:
     - pkg/alg/tree.go:TraverseTree (similar functionality)
     - Any custom recursive tree traversals

   Function: Transform / TransformInPlace
   Position: pkg/uast/pkg/node/node.go:413-428, 431-434
   Findings: Functional tree transformation patterns. Transform creates new tree, TransformInPlace mutates.
   Reusability: General tree transformation pattern.

   Function: Find (predicate-based search)
   Position: pkg/uast/pkg/node/node.go:287-299
   Findings: Pre-order search with predicate function. Returns matching nodes.
   Reusability: Generic tree search pattern.

   Function: ReleaseTree
   Position: pkg/uast/pkg/node/node.go:272-285
   Findings: Iterative tree memory cleanup using sync.Pool. Prevents memory leaks.
   Reusability: Memory cleanup pattern for tree structures.

   Function: AssignStableIDs
   Position: pkg/uast/pkg/node/node.go:578-600
   Findings: Content-based SHA1 hashing for stable node IDs. Deterministic ID generation.
   Reusability: Stable ID generation for any tree/graph structure.

   Function: Ancestors
   Position: pkg/uast/pkg/node/node.go:340-368
   Findings: Iterative ancestor path finding. Returns path from root to parent.
   Reusability: Tree navigation utility.

9. pkg/uast/pkg/node/allocator.go

   Function: Allocator (type)
   Position: pkg/uast/pkg/node/allocator.go:8-88
   Findings: Per-worker free-list allocator. Avoids sync.Pool cross-goroutine contention. GetNode/PutNode/GetPositions/PutPositions methods. ReleaseTree for cleanup.
   Reusability: Superior to sync.Pool for single-threaded or worker-local scenarios. General memory pool pattern.

10. pkg/uast/pkg/node/comparison.go

    Function: tokensCompare, parseFloat, compareFloatWithOp, compareStringWithOp
    Position: pkg/uast/pkg/node/comparison.go:6-42
    Findings: Basic comparison utilities for tokens (float or string). Simple but reusable.

11. pkg/uast/pkg/node/classifier.go

    Function: ClassifyDSLNode, isLiteralNode
    Position: pkg/uast/pkg/node/classifier.go:4-23
    Findings: DSL node type classification. Simple type switch pattern.

12. internal/analyzers/anomaly

    Function: ComputeZScores
    Position: internal/analyzers/anomaly/zscore.go:14-55
    Findings: Z-score computation with sliding window. Uses trailing window [max(0, i-window):i] to calculate mean/stddev, then computes (value-mean)/stddev. Handles edge case when stddev=0 by returning ZScoreMaxSentinel. Already uses pkg/alg/stats.MeanStdDev.
    Reusability: HIGH - Generic statistical anomaly detection pattern. Could be moved to pkg/alg/stats.
    Could replace:
      - Any ad-hoc z-score calculations in other analyzers

    Function: AggregateCommitsToTicks
    Position: internal/analyzers/anomaly/metrics.go:186-214
    Findings: Aggregates per-commit metrics into per-tick metrics using commits_by_tick mapping. Uses mapx.MergeAdditive for language maps. Generic aggregation pattern.
    Reusability: HIGH - Generic commit-to-tick aggregation pattern.
    Could replace:
      - Similar aggregation logic in other history analyzers (burndown, couples, file_history, devs)

    Function: aggregateTickFromCommits
    Position: internal/analyzers/anomaly/metrics.go:217-242
    Findings: Merges commit-level data for a single tick. Handles map merging, slice concatenation, and set accumulation.
    Reusability: MEDIUM - Tick aggregation helper.

    Function: ParseReportData
    Position: internal/analyzers/anomaly/metrics.go:245-274
    Findings: Type-safe report parsing with fallback to canonical format (commit_metrics + commits_by_tick). Extracts strongly-typed data from map[string]any.
    Reusability: HIGH - Report parsing pattern applicable to all analyzers.

    Function: computeTimeSeries
    Position: internal/analyzers/anomaly/metrics.go:124-179
    Findings: Builds annotated time series with anomaly flags and Z-scores. Uses O(1) anomaly set lookup. Reuses ComputeZScores for churn dimension.
    Reusability: MEDIUM - Time series annotation pattern.

    Function: detectExternalAnomalies
    Position: internal/analyzers/anomaly/enrich.go:10-68
    Findings: Generic anomaly detection over external time series dimensions. Iterates over sorted dimension names for determinism. Returns both individual anomalies and summary stats.
    Reusability: HIGH - Cross-analyzer anomaly detection pattern.

    Pattern: ExternalAnomaly/ExternalSummary types
    Position: internal/analyzers/anomaly/metrics.go:90-114
    Findings: Types for cross-analyzer anomaly reporting. Source + Dimension + Tick + ZScore + RawValue. Summary includes mean/stddev/anomaly count.
    Reusability: HIGH - Standardized cross-analyzer communication format.

    Pattern: ComputedMetrics interface implementation
    Position: internal/analyzers/anomaly/metrics.go:280-310
    BaseHistoryAnalyzer[M] - Embedded base implementation reducing boilerplate
    Findings: Implements AnalyzerName(), ToJSON(), ToYAML(), ComputeAllMetrics(). Standard pattern for metrics computation from report data.
    Reusability: HIGH - Already standardized via common.ComputedMetrics base.

    Pattern: Report section building (buildExternalAnomalySection)
    Position: internal/analyzers/anomaly/plot.go:91-122
    Findings: Builds grid of stat cards for external anomaly summaries. Uses plotpage.Stat with trend badges.
    Reusability: MEDIUM - Report visualization pattern.

13. internal/analyzers/burndown

    Function: PathInterner (type)
    Position: internal/analyzers/burndown/path_interner.go:17-59
    Findings: Thread-safe string interner mapping paths to stable numeric IDs (PathID). Uses RWMutex, sequential ID assignment (0,1,2...), slice-backed reverse lookup. Enables slice-indexed state instead of map[string] for better iteration performance.
    Reusability: HIGH - General string interning pattern for performance-critical map-to-slice conversion.
    Could replace:
      - Ad-hoc path/symbol interning in other performance-critical code

    Function: sparseHistory type
    Position: internal/analyzers/burndown/aggregator.go:25
    Findings: Type alias: map[int]map[int]int64. Represents 2D sparse matrix (tick x band -> line count). Used for memory-efficient delta accumulation.
    Reusability: MEDIUM - Sparse matrix pattern for time-series data.

    Function: mergeKeyedDeltas
    Position: internal/analyzers/burndown/aggregator.go:130-140 (implied usage)
    Findings: Merges nested delta maps using mapx.MergeNestedAdditive. Generic pattern for accumulating sparse history deltas.
    Reusability: MEDIUM - Delta accumulation pattern.

    Function: EstimatedStateSize
    Position: internal/analyzers/burndown/aggregator.go:330-350
    Findings: Memory footprint estimation for spill budget decisions. Uses constant per-entry estimates (sparseEntryBytes=56, matrixRowBytes=48). Walks nested maps to calculate size.
    Reusability: HIGH - Memory estimation pattern for spill/h251bernation decisions.

    Function: Spill/Collect pattern
    Position: internal/analyzers/burndown/aggregator.go:230-320
    Findings: Gob-based disk spilling with automatic cleanup. Spill() writes state, clears memory, increments spillN. Collect() reads all spills, merges back, cleans up temp dir. Supports custom spill dir or auto-temp.
    Reusability: HIGH - Generic spill-to-disk pattern for memory-bounded processing.

    Function: mergeAllTicks
    Position: internal/analyzers/burndown/aggregator.go:430-460
    Findings: Merges multiple TICK results into single accumulated state. Handles nested map merging for GlobalHistory, PeopleHistories, Matrix, FileHistories, FileOwnership.
    Reusability: MEDIUM - Tick merging pattern.

    Function: buildDenseMatrix
    Position: internal/analyzers/burndown/aggregator.go:530-570
    Findings: Converts sparse matrix []map[int]int64 to dense [][]int64. Handles column mapping (authorSelf->0, others->author+2). Generic sparse-to-dense conversion.
    Reusability: MEDIUM - Sparse-to-dense matrix conversion.

    Function: groupSparseHistory
    Position: internal/analyzers/burndown/history.go (implied)
    Findings: Converts sparse history to dense history matrix with configurable granularity/sampling. Core burndown aggregation algorithm.
    Reusability: LOW - Domain-specific but algorithm is general for cohort analysis.

    Function: ParseReportData
    Position: internal/analyzers/burndown/metrics.go:30-75
    Findings: Type-safe extraction of strongly-typed data from analyze.Report map. Handles all burndown report fields with defaults (TickSize defaults to 24h).
    Reusability: HIGH - Report parsing pattern (same as anomaly).

    Function: findPeakLines
    Position: internal/analyzers/burndown/metrics.go:100-120
    Findings: Computes total lines ever written by finding max value per band across all samples. Correct denominator for survival rate calculation.
    Reusability: MEDIUM - Peak-finding pattern for cohort analysis.

    Function: computeSurvivalSample
    Position: internal/analyzers/burndown/metrics.go:135-155
    Findings: Computes survival rate for single sample: sum(positive values) / peakLines. Returns breakdown per band.
    Reusability: MEDIUM - Survival rate calculation pattern.

    Function: computeFileSurvival
    Position: internal/analyzers/burndown/metrics.go:180-210
    Findings: Computes file ownership statistics: current lines, top owner ID/name/percentage. Iterates ownership map to find max.
    Reusability: MEDIUM - Ownership statistics pattern.

    Function: computeInteraction
    Position: internal/analyzers/burndown/metrics.go:250-290
    Findings: Extracts developer interaction data from PeopleMatrix. Maps modifier index to modifier ID (index-2), identifies self-modifies.
    Reusability: LOW - Domain-specific but interaction tracking pattern is general.

    Pattern: CommitResult/CommitSummary/TickResult types
    Position: internal/analyzers/burndown/tc.go:4-60
    Findings: Standard TC/TICK data carriers. CommitResult has deltas (GlobalDeltas, PeopleDeltas, MatrixDeltas, FileDeltas, FileOwnership) + derived LinesAdded/LinesRemoved. CommitSummary for timeseries. TickResult for aggregated state.
    Reusability: HIGH - Consistent with analyze.TC/TICK pattern.

    Pattern: Shard-based parallel processing
    Position: internal/analyzers/burndown/history.go:300-400 (processShardChanges)
    Findings: Partitions work by file name hash (FNV-32a), processes shards in parallel goroutines, collects renames separately for sequential handling. Uses sync.WaitGroup + error array.
    Reusability: HIGH - Sharded parallel processing with sequential epilogue pattern.

    Pattern: ChangeRouter usage
    Position: internal/analyzers/burndown/history.go:350-380
    Findings: Uses plumbing.ChangeRouter for type-specific change handling (OnInsert/OnDelete/OnModify/OnRename). Clean separation of change type logic.
    Reusability: HIGH - Already available in plumbing package.

    Pattern: ensureCapacity + swap-remove for slice growth
    Position: internal/analyzers/burndown/history.go:250-280
    Findings: Grows slice-backed state lazily (check cap before alloc). removeActiveID uses swap-remove (O(1) deletion). Performance optimization for hot paths.
    Reusability: HIGH - Slice performance pattern.

    Pattern: packPersonWithTick / unpackPersonWithTick
    Position: internal/analyzers/burndown/history.go:500-520
    Findings: Bit-packing two ints into one: tick in lower bits, person in upper bits. Uses burndown.TreeMaxBinPower for shift amount.
    Reusability: MEDIUM - Bit-packing pattern for memory efficiency.

14. internal/analyzers/clones

    Function: Shingler (type)
    Position: internal/analyzers/clones/shingler.go:17-58
    Findings: Extracts k-gram shingles from UAST function subtrees. Performs pre-order traversal, collects node types, creates sliding window of k consecutive types joined by separator. Generic k-gram extraction pattern applicable to any tree-based n-gram analysis.
    Reusability: HIGH - Generic k-gram extraction from trees. Could be moved to pkg/alg or pkg/uast for reuse in other n-gram based analyses (e.g., typos, sequence analysis).
    Could replace:
      - Any ad-hoc k-gram extraction in other analyzers

    Function: collectNodeTypes
    Position: internal/analyzers/clones/shingler.go:47-55
    Findings: Pre-order traversal collecting node types. Simple but reusable tree-to-sequence conversion.
    Reusability: MEDIUM - Tree-to-sequence extraction pattern.

    Function: Visitor pattern for deferred analysis
    Position: internal/analyzers/clones/visitor.go:11-100
    Findings: Collects function nodes during traversal, exports MinHash signatures for cross-file analysis by aggregator. Pattern: visit collects data, GetReport exports for aggregator.
    Reusability: HIGH - Deferred analysis pattern where per-file work is minimal, aggregator does heavy lifting.
    Could replace:
      - Similar patterns in other cross-file analyzers (couples, devs)

    Function: Aggregator pattern (cross-file analysis)
    Position: internal/analyzers/clones/aggregator.go:8-120
    Findings: Collects per-file signatures, builds global LSH index, finds cross-file clone pairs. Pattern: aggregate per-file data, build global index, query for relationships.
    Reusability: HIGH - Cross-file relationship detection pattern.
    Could replace:
      - Similar patterns in couples analyzer (file coupling)
      - devs analyzer (developer contributions across files)

    Function: qualifyFuncName
    Position: internal/analyzers/clones/aggregator.go:105-113
    Findings: Creates qualified name "sourceFile::name" for cross-file disambiguation. Simple but reusable naming pattern.
    Reusability: MEDIUM - Qualified name pattern for cross-file entity resolution.

    Function: findClonePairs with pairCap
    Position: internal/analyzers/clones/visitor.go:103-130
    Findings: Queries LSH index, collects unique pairs with seen map, supports pairCap for limiting stored results while tracking exact totalCount. Pattern: cap storage but track exact count.
    Reusability: HIGH - Bounded result storage with exact counting pattern.

    Function: clonePairKey (canonical pair key)
    Position: internal/analyzers/clones/report.go:125-133
    Findings: Creates canonical key by ordering alphabetically so (A,B) and (B,A) produce same key. Prevents duplicate pairs.
    Reusability: HIGH - Canonical pair key pattern for any symmetric relationship detection.
    Could replace:
      - Any ad-hoc pair deduplication logic

    Function: extractClonePairs (type-safe extraction)
    Position: internal/analyzers/clones/report.go:136-165
    Findings: Handles multiple representations: []ClonePair, []map[string]any, []any (JSON decoded). Comprehensive type-safe extraction.
    Reusability: HIGH - Already using common patterns, but extraction logic is thorough.

    Pattern: Signature export for cross-file analysis
    Position: internal/analyzers/clones/visitor.go:85-97 (keyFuncSignatures)
    Findings: Per-file report exports signatures with _source_file stamp. Aggregator reads and qualifies names. Clean separation of per-file vs global work.
    Reusability: HIGH - Pattern for other cross-file analyses (imports, couples, devs).

15. internal/analyzers/cohesion

    Function: Bloom filter optimization for variable membership (buildPerFunctionBloomFilters)
    Position: internal/analyzers/cohesion/calculations.go:75-93
    Findings: Creates Bloom filter per function's variable set for O(1) membership tests instead of O(M) linear scans. Reduces countVariableAccesses from O(N² × M) to O(N × M). Performance optimization pattern using Bloom filters for set membership in hot loops.
    Reusability: HIGH - General Bloom filter optimization pattern for repeated set membership tests.
    Could replace:
      - Any ad-hoc slices.Contains loops in performance-critical code

    Function: buildGlobalVariableFilter
    Position: internal/analyzers/cohesion/calculations.go:148-178
    Findings: Creates global Bloom filter containing only variables that appear in more than one function (truly shared). Filters out unique variables before building filter. Pattern: pre-filter data before building probabilistic structure.
    Reusability: MEDIUM - Pre-filtering pattern for Bloom filter construction.

    Function: calculateFunctionLevelCohesion (variable sharing ratio)
    Position: internal/analyzers/cohesion/calculations.go:130-146
    Findings: Computes cohesion as sharedVars / totalUniqueVars. Measures what fraction of function's variables are shared with other functions. Uses Bloom filter for O(1) membership. Generic cohesion calculation pattern.
    Reusability: HIGH - Variable sharing ratio pattern for measuring module cohesion.
    Could replace:
      - Ad-hoc cohesion calculations in other static analysis tools

    Function: LCOM-HS calculation (calculateLCOM)
    Position: internal/analyzers/cohesion/calculations.go:18-42
    Findings: Implements Henderson-Sellers LCOM formula: LCOM = 1 - sum(mA) / (m * a). Industry standard metric (used by NDepend, JArchitect, CppDepend). Already using pkg/alg/stats.Clamp.
    Reusability: HIGH - Standard software metric, already well-implemented.

    Function: ContextStack for nested traversal
    Position: internal/analyzers/cohesion/visitor.go:14-20, 58-82
    Findings: Uses common.ContextStack[*cohesionContext] for tracking nested function contexts during traversal. Push on function enter, pop on exit, collect function data. Pattern: context stack for nested scope tracking.
    Reusability: MEDIUM - Already using common.ContextStack, pattern is reusable for other nested traversals.

    Function: Typed report items with convert function (FunctionReportItem)
    Position: internal/analyzers/cohesion/cohesion.go:168-195
    Findings: Typed struct FunctionReportItem with convertCohesionFunctionItems for serialization to []map[string]any. FRD-compliant typed representation pattern. Includes source_file stamping.
    Reusability: HIGH - Already standardized via FRD, pattern is reusable for other typed report items.

    Function: Assessment labelers with emoji (getCohesionAssessment, getVariableAssessment, getSizeAssessment)
    Position: internal/analyzers/cohesion/cohesion.go:252-280
    Findings: Threshold-based assessment with emoji indicators (🟢/🟡/🔴). Separate assessments for cohesion, variable count, size. Reusable pattern for multi-dimensional assessment.
    Reusability: MEDIUM - Emoji assessment pattern, already using common thresholds.

    Function: Aggregator configuration builder (buildAggregatorConfig)
    Position: internal/analyzers/cohesion/aggregator.go:28-52
    Findings: Builds aggregatorConfig struct with typed builder functions (messageBuilder, emptyResultBuilder, numericKeys, countKeys). Clean separation of configuration from logic.
    Reusability: MEDIUM - Configuration builder pattern for common.Aggregator.

    Function: Distribution categorization with stats.Distribution
    Position: internal/analyzers/cohesion/metrics.go:165-178
    Findings: Uses stats.Distribution generic function for categorizing functions by cohesion level. Reuses pkg/alg/stats utility.
    Reusability: LOW - Already using reusable pkg/alg/stats function.

    Function: Box plot grouping by directory (groupByDirectory)
    Position: internal/analyzers/cohesion/plot.go:282-314
    Findings: Groups functions by source file directory, filters small groups (<3), sorts by median ascending (worst first), caps at maxDirectories. Pattern: group-by-dimension with statistical sorting and culling.
    Reusability: HIGH - General visualization grouping pattern for per-directory metrics.
    Could replace:
      - Similar grouping logic in other analyzers (complexity, quality)

    Function: shortenDirectory (path truncation)
    Position: internal/analyzers/cohesion/plot.go:317-333
    Findings: Keeps last N path components for display. Handles empty components, uses maxPathComponents limit. Reusable path shortening for UI display.
    Reusability: MEDIUM - Path truncation pattern for UI labels.

---

## Reusable packages (already in use)

The following pkg/alg packages are well-designed and already being reused internally:

- pkg/alg/cms/cms.go - Count-Min Sketch (used by halstead analyzer)
- pkg/alg/hll/hll.go - HyperLogLog (used by couples, devs analyzers)
- pkg/alg/interval/interval.go - Interval tree (used by burndown range_query)
- pkg/alg/iter.go - Iterator pattern (generic interface)
- pkg/alg/levenshtein/levenshtein.go - Edit distance (used by typos analyzer)
- pkg/alg/lru/cache.go - LRU cache with bloom filter (used by diff_cache, blob_cache)
- pkg/alg/lsh/lsh.go - LSH index (used by clones analyzer)
- pkg/alg/mapx/maps.go - Map utilities (CloneFunc, MergeAdditive, SortedKeys)
- pkg/alg/mapx/slices.go - Slice utilities (SortAndLimit, BuildLookupSet, Unique)
- pkg/alg/minhash/minhash.go - MinHash signatures (used by lsh, clones)
- pkg/alg/stats/stats.go - Statistical functions (Mean, StdDev, Percentile, ZScore, Distribution, Clamp)
- pkg/alg/pairs.go - Pair iteration utility

---

## Summary of Reusable Patterns Found

### Concurrency Patterns
- WorkerPool (pkg/pipeline) - Bounded concurrency worker pool
- RunPC (pkg/pipeline) - Producer-consumer goroutine skeleton
- SignalOnDrain (pkg/pipeline) - Channel draining utility
- SharedResponse (pkg/pipeline) - sync.Once caching with context
- Worker (pkg/gitlib) - Single-threaded CGEWORKER
- Shard-based parallel processing (burndown) - Partition by hash, parallel goroutines, sequential epilogue

### Memory Optimization Patterns
- sync.Pool usage (pkg/uast/pkg/node) - Memory pooling
- Allocator (pkg/uast/pkg/node) - Per-worker free-list allocator
- ReleaseTree (pkg/uast/pkg/node) - Tree memory cleanup
- PathInterner (burndown) - String interning for slice-backed state
- Bit-packing (burndown) - packPersonWithTick for memory efficiency
- Spill-to-disk (burndown, streaming) - Gob-based memory spilling
- Bloom filter optimization (cohesion) - O(1) set membership in hot loops

### Data Structure Patterns
- Bloom filter (pkg/alg/bloom) - Already complete, reusable
- LRU cache (pkg/alg/lru) - Already in use
- Tree traversal (pkg/alg/tree) - Iterative DFS
- SparseHistory (burndown) - Sparse matrix map[int]map[int]int64
- PathInterner (burndown) - String->ID interner with slice reverse lookup

### Type Conversion Patterns
- MustConvert/SafeConvert (pkg/safeconv) - Generic safe conversions
- Extract (pkg/safeconv) - Type assertion with numeric coercion

### File/IO Patterns
- ReadFile/ResolvePath (pkg/iosafety) - Safe file reading
- Filter (pkg/pathfilter) - File filtering with enry
- IsBinary/CountLines (pkg/textutil) - Text utilities
- Codec/Persister (pkg/persist) - Serialization abstraction
- Gob encoding (burndown aggregator) - State serialization for spill

### Time Patterns
- FloorTime (pkg/timeutil) - Time bucketing
- ParseTime (pkg/gitlib) - Multi-format time parsing

### Configuration Patterns
- ConfigurationOption (pkg/pipeline) - Unified config type
- BatchConfig (pkg/gitlib) - Batch processing config
- Option pattern (pkg/pathfilter) - Functional options
- Aggregator config builder (cohesion) - Typed configuration builder

### Metric/Analysis Patterns
- Metric interface (pkg/metrics) - Self-contained metric computation
- Registry (pkg/metrics) - Metric collection
- RiskPriority (pkg/metrics) - Risk sorting
- Survival rate calculation (burndown) - Peak lines / current lines
- Interaction tracking (burndown) - Author-modifier matrix
- LCOM-HS (cohesion) - Henderson-Sellers cohesion metric
- Variable sharing ratio (cohesion) - Function cohesion measurement

### Signal/Shutdown Patterns
- SignalCleanupGuard (pkg/sigutil) - Graceful shutdown
- SpillCleanupGuard (internal/streaming) - Signal-driven spill cleanup

### Tree/Graph Patterns
- Builder pattern (pkg/uast/pkg/node) - Fluent object construction
- Transform/TransformInPlace (pkg/uast/pkg/node) - Tree transformation
- Find with predicate (pkg/uast/pkg/node) - Tree search
- AssignStableIDs (pkg/uast/pkg/node) - Content-based hashing
- Ancestors (pkg/uast/pkg/node) - Tree navigation
- ContextStack (common) - Nested scope tracking during traversal

### Pipeline/Stage Patterns
- Batcher (pkg/pipeline) - Batch accumulation
- Phase/RunPhases (pkg/pipeline) - Chain of responsibility
- Fetcher (pkg/pipeline) - Cache decorator pattern base
- Aggregator Spill (burndown) - Memory-bounded accumulation

### Parsing Patterns
- PatternMatcher (pkg/uast/pkg/mapping) - Query caching with sync.Pool
- parseQuotedList (pkg/uast/pkg/mapping) - DSL parsing utility
- isValidIdentifier (pkg/uast/pkg/mapping) - Identifier validation
- ChangeRouter (plumbing) - Change type demultiplexing

### Utility Patterns
- KiB/MiB/GiB (pkg/units) - Binary size constants
- ReadRSSBytes (pkg/meminfo) - Memory monitoring
- WriteJSON (pkg/textutil) - JSON helper
- FNV hash sharding (burndown) - Deterministic partitioning
- Swap-remove deletion (burndown) - O(1) slice element removal
- ensureCapacity lazy growth (burndown) - Slice capacity optimization
- Path truncation (cohesion) - shortenDirectory for UI display
- Canonical pair key (clones) - Symmetric relationship deduplication
- k-gram extraction (clones) - Shingler for tree-based n-grams

---

## Progress Graph

[ pkg/alg [✅] ] -> [ pkg/iosafety [✅] ]
               -> [ pkg/uast [✅] ]
               -> [ pkg/pipeline [✅] ]
               -> [ pkg/safeconv [✅] ]
               -> [ pkg/gitlib [✅] ]
               -> [ pkg/meminfo [✅] ]
               -> [ pkg/textutil [✅] ]
               -> [ pkg/timeutil [✅] ]
               -> [ pkg/units [✅] ]
               -> [ pkg/pathfilter [✅] ]
               -> [ pkg/persist [✅] ]
               -> [ pkg/version [✅] ]
               -> [ pkg/metrics [✅] ]
               -> [ pkg/sigutil [✅] ]
               -> [ internal/analyzers/common [✅] ]
               -> [ internal/analyzers/analyze [✅] ]
               -> [ internal/analyzers/plumbing [✅] ]
               -> [ internal/streaming [✅] ]
               -> [ internal/checkpoint [✅] ]
               -> [ internal/analyzers/*other [✅] ] -> [ anomaly [✅] ]
                                                       -> [ burndown [✅] ]
                                                       -> [ clones [✅✅] ]
                                                       -> [ cohesion [⌛✅] ]
                                                       -> [ comments [⌛] ]
                                                       -> [ complexity [ ] ]
                                                       -> [ couples [ ] ]
                                                       -> [ devs [ ] ]
                                                       -> [ file_history [ ] ]
                                                       -> [ halstead [ ] ]
                                                       -> [ imports [ ] ]
                                                       -> [ quality [ ] ]
                                                       -> [ sentiment [ ] ]
                                                       -> [ shotness [ ] ]
                                                       -> [ typos [ ] ]
               -> [ internal/observability [ ] ]
               -> [ internal/framework [ ] ]
               -> [ internal/config [ ] ]
               -> [ internal/mcp [ ] ]
               -> [ internal/burndown [ ] ]
               -> [ internal/budget [ ] ]
               -> [ internal/plumbing [ ] ]
               -> [ internal/identity [ ] ]
               -> [ internal/importmodel [ ] ]
               -> [ internal/storage [ ] ]
               -> [ internal/cache [ ] ]
               -> [ cmd/* [ ] ]
               -> [ tools/* [ ] ]
               -> [ scripts/* [ ] ]
               -> [ examples/* [ ] ]

---

## Key Reusable Components from internal/analyzers/analyze

### Core Interfaces
- HistoryAnalyzer - History-based analysis contract with Consume, Serialize, Fork/Merge
- StaticAnalyzer - UAST-based static analysis contract
- CommitTimeSeriesProvider - Interface for contributing to unified timeseries output
- StoreWriter/DirectStoreWriter - Streaming record write interfaces for memory-bounded analysis
- Parallelizable - Worker pool execution support (SnapshotPlumbing, CPUHeavy, SequentialOnly)

### Reusable Types
- TC/TICK - Per-commit and aggregated tick result carriers
- TypedCollection - Deferred map conversion wrapper for memory efficiency
- GenericAggregator[S,T] - Generic per-tick state management with spilling support
- BaseHistoryAnalyzer[M] - Embedded base implementation reducing boilerplate
- MultiAnalyzerTraverser - Iterative multi-visitor UAST traversal
- MergeTracker - Bloom filter-based merge commit deduplication
- StreamingSink - Thread-safe NDJSON writer for pipeline output
- FileReportStore - File-backed streaming storage with gob encoding
- StaticService - High-level static analysis service facade

### Reusable Functions
- ReadRecordsIfPresent/ReadRecordIfPresent - Generic record reading helpers
- WriteSliceKind - Bulk record writing
- BuildCommitsByTick - Tick-to-commits extraction
- DrainCommitStatsHelper - Stats extraction pattern
- BuildMergedTimeSeriesDirect - Time-series data merging
- SafeMetricComputer - Defensive computation wrapper
- NormalizeFormat/ValidateFormat - Format handling utilities
- ExpandPatterns - Glob pattern expansion against registry

### Patterns Identified
- Plugin architecture with Registry and Descriptor
- Template method pattern via BaseHistoryAnalyzer
- Strategy pattern via delegate hooks in GenericAggregator
- Streaming storage abstraction (ReportWriter/ReportReader)
- Memory spilling for large data (SpillStore, UAST spill)
- Cross-format conversion via UnifiedModel
- Report normalization via ReportSection interface
- Pre-computation data carrier (PreparedCommit)
- Output multiplexing (OutputHistoryResults)

---

## New findings: internal/analyzers/plumbing

### Identity Detection Pattern
**Location**: internal/analyzers/plumbing/identity.go
**Components**:
- `IdentityDetector` - Maps commit authors to canonical developer identities
- Exact vs loose signature matching (email|name)
- Incremental dictionary building during Consume()
- `registerLooseIdentity` - Helper for linking emails and names
- `LoadPeopleDict` - File-based dictionary loading
- `GeneratePeopleDict` - Build from commits

**Reusability**: MEDIUM - Domain-specific but pattern is reusable for entity resolution

### Tick Calculation Pattern
**Location**: internal/analyzers/plumbing/ticks.go
**Components**:
- `TicksSinceStart` - Computes relative time ticks for commits
- Uses `pkg/timeutil.FloorTime` for bucketing
- Tracks commits per tick
- Handles merge commit deduplication

**Reusability**: LOW - Domain-specific time bucketing

### Fact Accessors
**Location**: internal/plumbing/fact_accessors.go
**Components**:
- `GetTickSize`, `GetCommitsByTick`, `GetReversedPeopleDict`, `GetPeopleCount`
- Type-safe extraction from facts map with (val, ok) pattern

**Reusability**: MEDIUM - Type-safe map access pattern

---

## New findings: internal/streaming

### Hibernation Pattern
**Location**: internal/streaming/hibernatable.go
**Components**:
- `Hibernatable` interface - Hibernate()/Boot() for state compression
- `SpillCleaner` interface - CleanupSpills() for temp file cleanup
- `SpillCleanupGuard` - Embeds sigutil.SignalCleanupGuard for signal-driven cleanup
- Ensures cleanup on normal exit, error exit, and SIGTERM/SIGINT

**Reusability**: HIGH - Graceful cleanup pattern with signal handling

### Memory Telemetry Logging
**Location**: internal/streaming/memlog.go
**Components**:
- `ChunkMemoryLog` - Memory measurements per chunk
- `LogChunkMemory` - Structured logging with units (MiB, KiB)
- Tracks heap, RSS, sys memory, budget usage, EMA growth rate

**Reusability**: MEDIUM - Structured telemetry logging pattern

---

## New findings: internal/checkpoint

### Checkpointable Interface
**Location**: internal/checkpoint/checkpointable.go
**Components**:
- `Checkpointable` - SaveCheckpoint/LoadCheckpoint/CheckpointSize
- Optional interface for analyzers supporting checkpointing

**Reusability**: HIGH - State persistence interface pattern

### Checkpoint Manager
**Location**: internal/checkpoint/manager.go
**Components**:
- `Manager` - Coordinates checkpoints across analyzers
- RepoHash - SHA256-based directory naming
- Metadata versioning (MetadataVersion = 2)
- Validation: repo path, analyzer list, version mismatch detection
- Retention: MaxAge (7 days), MaxSize (1GB)
- Uses pkg/persist for metadata serialization

**Reusability**: HIGH - Checkpoint coordination with validation

### Streaming State Tracking
**Location**: internal/checkpoint/state.go
**Components**:
- `StreamingState` - Chunk orchestrator progress (total/processed commits, current/total chunks)
- `AggregatorSpillEntry` - On-disk spill state (dir, count)
- `Metadata` - Checkpoint metadata with checksums

**Reusability**: MEDIUM - Progress tracking pattern

### Persister Alias
**Location**: internal/checkpoint/persister.go
**Components**:
- `Persister[T]` - Alias for persist.Persister[T]
- Re-exports pkg/persist functionality

**Reusability**: LOW - Re-export pattern

---

## Recommended Next Steps

### Immediate Dedup Opportunities
1. **Replace pkg/uast/loader.go bloom functions** with `pkg/alg/bloom/bloom.go`
2. **Replace ad-hoc type assertions** with `pkg/safeconv` generics
3. **Replace ad-hoc file reading** with `pkg/iosafety`
4. **Replace inline chunk loops** with `pkg/alg/chunk.go`
5. **Replace ThresholdLabeler** with `Classifier[float64]`
6. **Replace hardcoded 1024 constants** with `pkg/units`
7. **Replace ad-hoc slices.Contains loops** with Bloom filter optimization (cohesion pattern)
8. **Replace ad-hoc path shortening** with shortenDirectory pattern

### High-Value Reusable Patterns (Consider Extraction)
1. **SpillCleanupGuard** (internal/streaming) - Signal-driven cleanup pattern
2. **Checkpointable** (internal/checkpoint) - State persistence interface
3. **Checkpoint Manager** (internal/checkpoint) - Validation and coordination
4. **Hibernatable** (internal/streaming) - State compression interface
5. **Identity Detector pattern** (internal/analyzers/plumbing) - Entity resolution
6. **PathInterner** (internal/analyzers/burndown) - String interning for slice-backed state
7. **Spill-to-disk pattern** (internal/analyzers/burndown) - Gob-based memory spilling
8. **Shard-based parallel processing** (internal/analyzers/burndown) - Hash partitioning with sequential epilogue
9. **Shingler** (internal/analyzers/clones) - k-gram extraction from trees
10. **Canonical pair key** (internal/analyzers/clones) - Symmetric relationship deduplication
11. **Bloom filter optimization** (internal/analyzers/cohesion) - O(1) set membership in hot loops
12. **Variable sharing ratio** (internal/analyzers/cohesion) - Function cohesion measurement
13. **Box plot grouping by directory** (internal/analyzers/cohesion) - Per-directory statistical visualization
14. **ContextStack for nested traversal** (internal/analyzers/cohesion using common) - Nested scope tracking

### Already Well-Designed (No Action Needed)
- All `pkg/alg/*` packages
- `pkg/pipeline/*` packages
- `pkg/metrics/metrics.go`
- `pkg/sigutil/sigutil.go`
- `pkg/pathfilter/pathfilter.go`
- `pkg/persist/persist.go`
- `pkg/iosafety/iosafety.go`
- `pkg/safeconv/*`
- `internal/analyzers/common/*` (ContextStack, UASTTraverser, DataExtractor, Classifier, ThresholdLabeler)

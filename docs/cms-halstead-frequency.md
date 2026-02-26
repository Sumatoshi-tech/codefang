# Count-Min Sketch for Halstead Frequency Counting

The Halstead analyzer uses Count-Min Sketch (CMS) to provide streaming
total frequency counting alongside exact `map[string]int` frequency maps.
CMS supplements the exact maps — it does not replace them.

## Why CMS for Halstead

Exact operator/operand frequency maps serve two purposes:
- **Distinct counting**: `len(Operators)` and `len(Operands)` — needed for
  Vocabulary, EstimatedLength, Difficulty.
- **Total counting**: `SumMap(Operators)` and `SumMap(Operands)` — needed for
  Length, Volume, Effort, TimeToProgram, DeliveredBugs.

CMS provides O(1) total count retrieval via `TotalCount()`, replacing O(N)
map iteration in `SumMap()`. For large functions with thousands of tokens,
this enables streaming total counting without map iteration.

Key insight: `cms.Sketch.TotalCount()` returns the **exact** sum of all
additions, not an estimate. It is maintained as a simple `int64` counter
incremented on each `Add()`. This means CMS-backed totals are identical
to `SumMap()`.

## Integration Points

| Component | Change |
|---|---|
| `FunctionHalsteadMetrics.OperatorSketch` | `*cms.Sketch` tracking operator frequency via streaming `Add` |
| `FunctionHalsteadMetrics.OperandSketch` | `*cms.Sketch` tracking operand frequency via streaming `Add` |
| `FunctionHalsteadMetrics.EstimatedTotalOperators` | `int64` from `OperatorSketch.TotalCount()` |
| `FunctionHalsteadMetrics.EstimatedTotalOperands` | `int64` from `OperandSketch.TotalCount()` |
| `Metrics.EstimatedTotalOperators` | `int64` aggregated from function-level estimates |
| `Metrics.EstimatedTotalOperands` | `int64` aggregated from function-level estimates |

## How It Works

### Token Threshold

Functions with fewer than 1000 total tokens (operators + operands) use
the exact map path only. CMS sketches are allocated eagerly in `pushContext`
but nil'd out in `popContext` if the threshold is not met.

For functions meeting the threshold:
1. CMS sketches retain their accumulated counts.
2. `EstimatedTotalOperators = OperatorSketch.TotalCount()`
3. `EstimatedTotalOperands = OperandSketch.TotalCount()`

### Visitor Path

During AST traversal via `Visitor`:
1. `pushContext` creates `Operators`/`Operands` maps AND `OperatorSketch`/`OperandSketch` CMS sketches.
2. `recordOperator` increments `Operators[key]++` AND calls `OperatorSketch.Add([]byte(key), 1)`.
3. `recordOperand` increments `Operands[key]++` AND calls `OperandSketch.Add([]byte(key), 1)`.
4. `popContext` checks total tokens against threshold, populates estimated fields if above.

### Direct Analyzer Path

In `Analyzer.calculateFunctionHalsteadMetrics`:
1. Maps are populated via `CollectOperatorsAndOperands`.
2. If total tokens >= threshold, `populateCMSSketches` bulk-loads the CMS from maps.
3. Estimated fields populated from `TotalCount()`.

### File-Level Aggregation

`calculateFileLevelMetrics` sums function-level `EstimatedTotalOperators`
and `EstimatedTotalOperands` to produce file-level estimates.

## CMS Parameters

| Parameter | Value | Effect |
|---|---|---|
| `cmsEpsilon` | 0.001 | Width ~2719 columns |
| `cmsDelta` | 0.01 | Depth 5 rows |
| `cmsTokenThreshold` | 1000 | Minimum tokens for CMS activation |

Memory per sketch: width * depth * 8 bytes = ~108 KB. Two sketches per
function context = ~216 KB overhead (only for large functions).

## JSON Output

New fields are additive in JSON/YAML output:

```json
{
  "functions": [{
    "name": "processData",
    "total_operators": 500,
    "total_operands": 700,
    "estimated_total_operators": 500,
    "estimated_total_operands": 700,
    "distinct_operators": 15,
    "distinct_operands": 80,
    ...
  }],
  "estimated_total_operators": 500,
  "estimated_total_operands": 700
}
```

Existing consumers reading `total_operators` and `total_operands` are
unaffected. New consumers can use `estimated_*` fields.

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 with 10K-token functions:

| Metric | Value |
|---|---|
| CMS path (10K tokens) | ~2.07 ms/op, 231 KB, 23 allocs |
| Exact path (50 tokens) | ~38 μs/op, 231 KB, 23 allocs |
| CMS `Add` overhead | ~77 ns per token |
| CMS `TotalCount` | O(1) |

The CMS `Add` cost (~77 ns) is dominated by AST traversal overhead.
The primary benefit is O(1) `TotalCount()` for streaming scenarios.

## Design Decisions

1. **Supplement, not replace**: Exact maps are needed for distinct counting,
   report output (per-token frequency), and backward compatibility. CMS
   provides streaming total counts as an additive feature.

2. **Token threshold**: 1000 tokens avoids CMS overhead for small functions
   where `SumMap()` is trivially fast.

3. **Eager allocation, lazy retention**: CMS sketches are created in
   `pushContext` for all functions but nil'd out in `popContext` for small
   functions. This avoids conditional logic in the recording path.

4. **TotalCount is exact**: Unlike CMS `Count(key)` which may overestimate,
   `TotalCount()` is a simple counter — no approximation error.

5. **Dual-path consistency**: Both Visitor and direct Analyzer paths use the
   same CMS integration, ensuring consistent behavior regardless of traversal
   strategy.

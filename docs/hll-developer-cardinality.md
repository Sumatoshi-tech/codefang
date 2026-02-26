# HyperLogLog for Developer Cardinality

The devs analyzer uses HyperLogLog (HLL) sketches to provide approximate
developer cardinality estimates alongside exact counts. HLL supplements
the exact `map[int]*DeveloperData` — it does not replace it.

## Why HLL for Developer Counting

Exact developer counting requires materializing full maps of developer
IDs. For aggregate statistics like "how many unique developers contributed
to this project?" or "how many developers were active recently?", the
exact map is overkill — approximate counts are sufficient.

HLL provides:
- O(1) cardinality estimation from a fixed 16 KB sketch
- O(registers) merging for shard-parallel analysis
- ~0.8% standard error at precision p=14

## Integration Points

| Component | Change |
|---|---|
| `TickData.DevSketch` | `*hll.Sketch` built during `ParseTickData` from all developer IDs |
| `AggregateData.EstimatedTotalDevelopers` | `uint64` from `DevSketch.Count()` |
| `AggregateData.EstimatedActiveDevelopers` | `uint64` from active-threshold sketch |
| `devIDBytes(id int) []byte` | Helper converting developer ID to bytes via `strconv.AppendInt` |
| `buildDevSketch(ticks)` | Builds HLL sketch from tick developer IDs |
| `buildTotalDevSketch(developers)` | Builds HLL sketch from developer list |
| `buildActiveDevSketch(ticks, threshold)` | Builds HLL sketch from recent ticks |

## How It Works

### During `ParseTickData`

After aggregating commit data into per-tick per-developer maps, the
function builds an HLL sketch by iterating all ticks and adding each
developer ID (as bytes) to the sketch. If no ticks exist, `DevSketch`
is nil.

### During `AggregateMetric.Compute`

Two separate HLL sketches are built:
1. **Total sketch**: from the `Developers` slice (all developers).
2. **Active sketch**: from developer IDs in ticks at or above the
   active threshold (same threshold logic as exact counting).

The `Count()` of each sketch populates the estimated fields.

## Developer ID Encoding

Developer IDs are converted to bytes using `strconv.AppendInt`:

```go
func devIDBytes(id int) []byte {
    return strconv.AppendInt(nil, int64(id), devIDBase)
}
```

This is deterministic, handles all integer values (including large
IDs like `identity.AuthorMissing = 262142`), and produces unique
byte sequences for distinct IDs.

## JSON Output

The new fields are additive in the JSON/YAML output:

```json
{
  "aggregate": {
    "total_commits": 150,
    "total_developers": 12,
    "active_developers": 8,
    "estimated_total_developers": 12,
    "estimated_active_developers": 8,
    "analysis_period_ticks": 200,
    "project_bus_factor": 3,
    "total_languages": 5
  }
}
```

Existing consumers that read `total_developers` and `active_developers`
are unaffected. New consumers can use the `estimated_*` fields.

## Accuracy

HLL with p=14 provides ~0.8% standard error. For typical project
sizes:

| Developers | Expected Error | Absolute Error |
|---|---|---|
| 10 | <1% | <1 |
| 100 | ~0.8% | ~1 |
| 1,000 | ~0.8% | ~8 |
| 10,000 | ~0.8% | ~80 |

For very small cardinalities (1-10 developers), HLL is exact due
to the LogLog-Beta bias correction.

## Performance

Benchmarks on AMD Ryzen AI 9 HX 370 with 5000 developers:

| Metric | Value |
|---|---|
| `AggregateMetric.Compute` with HLL | ~1.0 ms/op |
| `AggregateMetric.Compute` without HLL | ~1.0 ms/op |
| HLL overhead | negligible (~19 ns per `Add`) |
| HLL memory | 16 KB per sketch |

The HLL `Add` cost (~19 ns) is negligible compared to the map
operations, sorting, and bus factor computation in `Compute`.

## Design Decisions

1. **Supplement, not replace**: Exact maps are needed for bus factor,
   rankings, language breakdowns, and per-developer display. HLL
   provides fast cardinality estimates as an additive feature.

2. **Precision p=14**: Standard choice — 16 KB memory, ~0.8% error.
   Good balance for developer counts from 1 to 100K.

3. **Sketch in TickData**: Placed here because TickData is the central
   input for all metric computations. One sketch covers all developer
   IDs encountered during parsing.

4. **Separate active sketch**: Built during `AggregateMetric.Compute`
   rather than stored in TickData, because the active threshold depends
   on the full tick range which is not known during parsing.

5. **Nil sketch for empty data**: When no ticks exist, `DevSketch` is
   nil. All consumers check for nil before calling `Count()`.

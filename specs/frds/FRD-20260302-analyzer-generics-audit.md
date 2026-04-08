# FRD: Analyzer Generics Audit & marshalAndWrite Promotion (Roadmap F3.3)

**ID**: FRD-20260302-analyzer-generics-audit
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F3.3

## Problem

The codebase has two powerful generic abstractions — `BaseHistoryAnalyzer[M]` and
`GenericAggregator[S,T]` — but adoption is incomplete. Three analyzers (burndown, couples,
file_history) still use custom aggregator implementations. Additionally, `WriteConvertedOutput`
in `conversion.go` manually marshals YAML instead of using the existing `marshalAndWrite`
helper. See LIST.md #25, #26.

## Feature

Complete the audit of all 10 history analyzers for generic adoption. Document which custom
aggregators can migrate to `GenericAggregator[S,T]` and which cannot (with rationale).
Promote `marshalAndWrite` to the one remaining manual marshal+write path in
`WriteConvertedOutput`.

## Audit Results

### BaseHistoryAnalyzer[M] Adoption — 10/10 (100%)

All history analyzers embed `*analyze.BaseHistoryAnalyzer[M]`:

| Analyzer | M Type | Status |
|----------|--------|--------|
| anomaly | `*ComputedMetrics` | Adopted |
| burndown | `*ComputedMetrics` | Adopted |
| couples | `*ComputedMetrics` | Adopted |
| devs | `*ComputedMetrics` | Adopted |
| file_history | `*ComputedMetrics` | Adopted |
| imports | `*ComputedMetrics` | Adopted |
| quality | `*ComputedMetrics` | Adopted |
| sentiment | `*ComputedMetrics` | Adopted |
| shotness | `*ComputedMetrics` | Adopted |
| typos | `*common.MetricSet` | Adopted |

### GenericAggregator[S,T] Adoption — 7/10

| Analyzer | Uses Generic? | S, T Types |
|----------|--------------|------------|
| anomaly | YES | `*tickAccumulator, *TickData` |
| devs | YES | `*TickDevData, *TickDevData` |
| imports | YES | `*tickAccumulator, *TickData` |
| quality | YES | `*tickAccumulator, *TickData` |
| sentiment | YES | `*tickAccumulator, *TickData` |
| shotness | YES | `*TickData, *TickData` |
| typos | YES | `*TickData, *TickData` |
| burndown | NO — custom | See rationale below |
| couples | NO — custom | See rationale below |
| file_history | NO — custom | See rationale below |

### Custom Aggregator Analysis

**burndown** — Cannot migrate. Manages 5+ heterogeneous state types (globalHistory,
peopleHistories, matrix, fileHistories, fileOwnership) with different merge semantics.
fileOwnership uses replacement semantics (point-in-time snapshots), not delta accumulation.
Custom spill serializes multiple independent data structures via Gob. The accumulation
pattern is global (across all ticks), not per-tick as GenericAggregator assumes. ~300 lines
of domain-specific logic.

**couples** — Cannot migrate. Multi-field accumulation (files SpillStore, people maps, bloom
filter pre-filtering) with domain-specific pruning and capping during spill collection
(`collectFilteredFiles`). Incremental map pre-allocation for large commits. The
`CollectWith` callback applies sophisticated filtered merges that have no generic equivalent.
~500 lines of domain-specific logic.

**file_history** — Cannot migrate. Although simpler than burndown/couples, the aggregator
processes complex path actions (Insert/Modify/Delete/Rename with path-aware mutations and
rename propagation) that are tightly coupled to the `Add()` method. The merge function
combines nested structures (People maps + Hashes slices) with domain-specific semantics.
Wrapping all state into a single S type would lose semantic clarity without reducing
complexity. ~200 lines of domain-specific logic.

**Conclusion:** All three custom aggregators manage fundamentally different accumulation
patterns than GenericAggregator's per-tick `map[int]S` model. Migration would require
either wrapping heterogeneous state into a single opaque type (losing clarity) or
extending GenericAggregator with features that defeat its genericity. The custom
implementations are justified and should remain.

### marshalAndWrite Promotion

One manual marshal+write pattern exists in `WriteConvertedOutput` (conversion.go:311-322)
for the YAML case. This can be replaced with the existing `marshalAndWrite` helper. The
JSON case uses `json.NewEncoder` with `SetIndent` (streaming with pretty-printing), which
is a different pattern and should remain as-is.

## Acceptance Criteria

- [x] Audit complete: documented which analyzers use generics and which still have custom implementations
- [x] Custom aggregators documented why they cannot migrate
- [x] `marshalAndWrite` usage promoted in `WriteConvertedOutput` YAML path
- [x] All tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Minimal.** The only code change is replacing a 7-line manual YAML marshal+write pattern
with a single `marshalAndWrite` call — identical behavior, better consistency. The audit
is documentation-only.

## Non-Goals

- Migrating burndown, couples, or file_history aggregators (documented as infeasible).
- Changing GenericAggregator to support new accumulation patterns.
- Modifying plumbing analyzers (they use streaming `json.NewEncoder`, appropriate for their use case).

## Implementation

### Files Modified

- `internal/analyzers/analyze/conversion.go` — YAML case in `WriteConvertedOutput` replaced with `marshalAndWrite` call

### Verification

- `go vet ./...` — clean
- `go test ./internal/analyzers/analyze/...` — all pass
- `make lint` — 0 issues, 0 dead code

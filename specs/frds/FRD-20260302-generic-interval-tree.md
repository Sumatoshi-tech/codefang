# FRD: Generic Interval Tree (Roadmap 4.3)

**ID**: FRD-20260302-generic-interval-tree
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 4.3
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 8, LIST #4

## Problem

`pkg/alg/interval/interval.go` hardcodes `uint32` for all three fields of `Interval` (Low, High, Value) and throughout the tree implementation. This prevents reuse with different integer types (e.g., `int`, `int64`) and forces call sites to cast:

```go
// internal/burndown/range_query.go — forced uint32 casts
low := uint32(offset)
high := uint32(offset + length - 1)
tree.Insert(low, high, t)
intervals := file.index.tree.QueryOverlap(uint32(startLine), uint32(endLine-1))
```

## Solution

Parameterize the interval tree with two type parameters: `K` for interval endpoints (Low, High, maxHigh) and `V` for the associated value.

### Type constraint

```go
// Integer constrains interval endpoints to integer types.
type Integer interface {
    ~int | ~int8 | ~int16 | ~int32 | ~int64 |
    ~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 | ~uintptr
}
```

Defined locally in the package (same pattern as `pkg/alg/lru` and `internal/config/facts.go`), avoiding a dependency on `golang.org/x/exp/constraints`.

### API changes

```go
// Before (non-generic):
type Interval struct { Low, High, Value uint32 }
type Tree struct { ... }
func New() *Tree
func (t *Tree) Insert(low, high, value uint32)
func (t *Tree) Delete(low, high, value uint32) bool
func (t *Tree) QueryOverlap(low, high uint32) []Interval
func (t *Tree) QueryPoint(point uint32) []Interval

// After (generic):
type Interval[K Integer, V comparable] struct { Low, High K; Value V }
type Tree[K Integer, V comparable] struct { ... }
func New[K Integer, V comparable]() *Tree[K, V]
func (t *Tree[K, V]) Insert(low, high K, value V)
func (t *Tree[K, V]) Delete(low, high K, value V) bool
func (t *Tree[K, V]) QueryOverlap(low, high K) []Interval[K, V]
func (t *Tree[K, V]) QueryPoint(point K) []Interval[K, V]
func (t *Tree[K, V]) Len() int
func (t *Tree[K, V]) Clear()
```

### Key design decisions

1. **`V comparable`** (not `V any`): `Delete` requires exact value matching (`==`). All practical value types (integers, strings, structs with comparable fields) satisfy `comparable`.

2. **Local `Integer` constraint**: Avoids external dependency. Only comparison operators (`<`, `>`, `<=`, `>=`, `!=`) are used on `K` — no arithmetic. The constraint could be broadened to `cmp.Ordered` later if float intervals are needed.

3. **Internal types parameterized**: `node[K, V]`, `Interval[K, V]`, `fixupResult` (unchanged — no type params needed), `color` (unchanged).

4. **Free functions gain type parameters**: `compareIntervals[K, V]`, `nodeColor[K, V]`, `setBlack[K, V]`, `childOf[K, V]`, `recalcMaxHigh[K, V]`, `updateMaxHigh[K, V]`, `minimum[K, V]`, `detachFromParent[K, V]`.

### Call site migration

Only one call site: `internal/burndown/range_query.go`.

```go
// Before:
tree  *interval.Tree
interval.New()

// After:
tree  *interval.Tree[uint32, uint32]
interval.New[uint32, uint32]()
```

## Acceptance Criteria

- [x] `Integer` constraint defined in `pkg/alg/interval/`
- [x] `Tree[K, V]`, `Interval[K, V]` parameterized with `K Integer, V comparable`
- [x] All existing 25 tests updated and passing (using `uint32`)
- [x] New test cases for at least 2 additional key types (`int`, `int64`)
- [x] `internal/burndown/range_query.go` updated to use generic types
- [x] `go test ./pkg/alg/interval/... ./internal/burndown/...` passes
- [x] `go vet` clean
- [x] `make lint` passes — zero issues, zero dead code
- [x] Existing benchmarks updated and passing

## Risk

Low. Mechanical type parameterization with no algorithmic changes. Single call site. Comprehensive existing test suite (25 tests + 4 benchmarks) validates behavior preservation.

## Implementation

### Files modified

| File | Change |
|------|--------|
| `pkg/alg/interval/interval.go` | Parameterized all types with `[K Integer, V comparable]`: `Tree`, `Interval`, `node`, and all free functions |
| `pkg/alg/interval/interval_test.go` | Updated all 25 tests to use `[uint32, uint32]`; added `TestGeneric_IntKeys` and `TestGeneric_Int64Keys` |
| `pkg/alg/interval/benchmark_test.go` | Updated all 4 benchmarks to use `[uint32, uint32]` |
| `internal/burndown/range_query.go` | Updated `rangeIndex.tree` to `*interval.Tree[uint32, uint32]` and `interval.New[uint32, uint32]()` |

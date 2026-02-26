# Halstead Allocation Hotspot Fix

The Halstead detector (`internal/analyzers/halstead/detector.go`) previously
allocated 5 maps and 1 slice on every node visit. This fix hoists all
constant data structures to package-level variables, eliminating per-call
overhead.

## Problem

The original `IsOperator`, `IsOperand`, `isDeclarationIdentifier`, and
`extractOperatorFromToken` functions each created local maps or slices
containing compile-time constant data:

| Function | Allocation | Entries |
|---|---|---|
| `IsOperator` | `operatorTypes` map | 7 `node.Type` entries |
| `IsOperator` | `operatorRoles` map | 4 `node.Role` entries |
| `IsOperand` | `operandTypes` map | 3 `node.Type` entries |
| `IsOperand` | `operandRoles` map | 4 `node.Role` entries |
| `isDeclarationIdentifier` | `declarationTypes` map | 12 `node.Type` entries |
| `extractOperatorFromToken` | `operators` slice | 33 string entries |

For a function with 1000 nodes, this creates ~5000 map constructions
per function. While the Go compiler optimizes small map literals to avoid
heap allocations, the CPU cost of constructing and hashing into fresh
maps each call was significant.

## Solution

All constant maps are hoisted to package-level `var` declarations:

- `operatorTypes` — maps operator UAST types
- `operatorRoles` — maps operator UAST roles
- `operandTypes` — maps operand UAST types
- `operandRoles` — maps operand UAST roles
- `declarationTypes` — maps declaration UAST types
- `tokenOperatorSet` — maps exact operator tokens for O(1) lookup
- `tokenOperatorsByLength` — sorted slice (longest-first) for containment matching

The `extractOperatorFromToken` function was additionally optimized:
the exact-match path now uses a map lookup (O(1)) instead of iterating
a 33-element slice. The containment path (`strings.Contains`) retains
the sorted slice for correct longest-first matching.

## Performance Results

Benchmarks on AMD Ryzen AI 9 HX 370 with -count=5:

| Benchmark | Before (ns/op) | After (ns/op) | Speedup |
|---|---|---|---|
| `IsOperator` (type match) | 84 | 5.2 | **16.2x** |
| `IsOperator` (role match) | 128 | 11.5 | **11.1x** |
| `IsOperator` (no match) | 123 | 11.6 | **10.6x** |
| `IsOperand` (type match) | 35 | 5.2 | **6.7x** |
| `IsOperand` (role match) | 83 | 11.3 | **7.3x** |
| `extractOperatorFromToken` (exact) | 403 | 7.9 | **51x** |
| `extractOperatorFromToken` (containment) | 320 | 324 | ~1x |
| `extractOperatorFromToken` (no match) | 425 | 383 | ~1.1x |
| Full traversal (1000 nodes) | 125,639 | 24,013 | **5.2x** |

All benchmarks report 0 B/op and 0 allocs/op (both before and after).

## Design Decisions

1. **Package-level `var` (not `const`)**: Go does not support `const`
   maps. Package-level `var` maps are initialized once at program start
   and read-only thereafter.

2. **Read-only safety**: These maps are never written to after
   initialization. Concurrent read access from multiple goroutines is
   safe without synchronization.

3. **Two-phase operator token matching**: Exact match via
   `tokenOperatorSet` (O(1)) is tried first. Only if that fails does the
   containment check iterate `tokenOperatorsByLength` with
   `strings.Contains`. The slice is sorted longest-first so "===" matches
   before "==".

4. **No behavioral change**: All 88 existing Halstead tests pass
   unchanged. The optimization is purely a latency improvement.

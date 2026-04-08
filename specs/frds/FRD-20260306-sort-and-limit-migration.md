# FRD: Replace Top-N patterns with mapx.SortAndLimit (Roadmap 1.6)

**ID**: FRD-20260306-sort-and-limit-migration
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 1.6
**Date**: 2026-03-06

## Problem

Seven `report_section.go` files each implement Top-N independently:

```go
// Pattern found in all 7 files (minor variants):
sort.Slice(items, func(i, j int) bool { return less(items[i], items[j]) })
if n >= len(items) { return items }
return items[:n]
```

`mapx.SortAndLimit[T]` already provides sort+limit atomically. These inline patterns
duplicate its behaviour and import `"sort"` directly.

## Decision

Replace all 7 patterns with `mapx.SortAndLimit`. Strategy per file:

### Group A — sort on `[]analyze.Issue` directly
Files: **cohesion**, **comments**, **couples**
- Extract unsorted `buildIssues()` (remove sort from old `buildSortedIssues`)
- `TopIssues(n)` → `mapx.SortAndLimit(s.buildIssues(), lessFunc, n)`
- `AllIssues()` → `mapx.SortAndLimit(s.buildIssues(), lessFunc, 0)` (`0` = no limit)

### Group B — sort on intermediate type, then build issues
Files: **clones** (ClonePair), **halstead** (map[string]any), **complexity** (issueEnvelope),
**imports** (importEntry / analyze.Issue)
- Unify `TopIssues` + `AllIssues` into a single `xyzIssues(limit int)` helper
- `mapx.SortAndLimit` applied to the intermediate type with `limit`
- Issues built from the limited sorted result

## `SortAndLimit` contract for limit=0

`SortAndLimit(items, less, 0)` returns all items sorted — no truncation.
This is the "AllIssues" semantic. Adding an explicit test to document this.

## Scope

### Files modified

| File | Change |
|------|--------|
| `pkg/alg/mapx/slices_test.go` | Add `limit_zero_returns_all` test |
| `internal/analyzers/cohesion/report_section.go` | Group A |
| `internal/analyzers/comments/report_section.go` | Group A |
| `internal/analyzers/couples/report_section.go` | Group A |
| `internal/analyzers/clones/report_section.go` | Group B; add test file |
| `internal/analyzers/complexity/report_section.go` | Group B |
| `internal/analyzers/halstead/report_section.go` | Group B |
| `internal/analyzers/imports/report_section.go` | Group B |
| `specs/ref/ROADMAP.md` | Mark 1.6 done |

## Acceptance Criteria

- [ ] All 7 `sort.Slice` calls inside `buildSortedIssues`/`buildIssues` replaced by `mapx.SortAndLimit`
- [ ] All 7 manual `if n >= len(issues) { ... } return issues[:n]` patterns eliminated
- [ ] `sort` stdlib import removed from all 7 files
- [ ] `mapx` import added to all 7 files
- [ ] All existing tests pass unchanged
- [ ] New test `limit_zero_returns_all` added to slices_test.go
- [ ] New test file added for `clones/report_section.go`
- [ ] `make lint` passes
- [ ] `make test` passes

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `pkg/alg/mapx/slices_test.go` | Added `limit_zero_returns_all` test |
| `internal/analyzers/cohesion/report_section.go` | Group A — `cohesionLess`, `buildIssues()` unsorted |
| `internal/analyzers/comments/report_section.go` | Group A — `commentNameLess`, `buildIssues()` unsorted |
| `internal/analyzers/couples/report_section.go` | Group A — `couplesValueLess`, renamed to `buildIssues()` |
| `internal/analyzers/clones/report_section.go` | Group B — `clonePairLess`, unified `cloneIssues(limit int)` |
| `internal/analyzers/clones/report_section_test.go` | New test file |
| `internal/analyzers/complexity/report_section.go` | Group B — `issueEnvelope` promoted, `complexityEnvelopeLess`, `complexityIssues(limit int)` |
| `internal/analyzers/halstead/report_section.go` | Group B — `halsteadFuncLess`, unified `halsteadIssues(limit int)` |
| `internal/analyzers/imports/report_section.go` | Group B — `importEntryLess`, `importNameLess`, unified `importIssues(limit int)` |
| `specs/ref/ROADMAP.md` | Marked 1.6 done |

## Risk

Low. The only subtle points:
- `SortAndLimit(nil, less, n)` → nil — matches `buildSortedIssues()` returning nil for empty input
- `SortAndLimit(items, less, 0)` → all items sorted — matches `AllIssues()` semantics
- **Cohesion** sort: string ascending on FormatFloat values in [0,1] — correct ✓
- **Couples** sort: string descending on formatted coupling strings — preserves existing (string-based) sort ✓
- **Complexity** sort: numeric via envelope struct — must remain numeric, not string ✓

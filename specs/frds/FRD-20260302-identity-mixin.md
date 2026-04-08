# FRD: IdentityMixin (Roadmap 3.4)

**ID**: FRD-20260302-identity-mixin
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 3.4
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 3b, LIST #8

## Problem

Four analyzer packages — burndown, couples, imports, devs — each contain an identical `getReversedPeopleDict()` method that resolves the reversed people dictionary with the same two-tier logic:

```go
func (x *Type) getReversedPeopleDict() []string {
    if x.Identity != nil && len(x.Identity.ReversedPeopleDict) > 0 {
        return x.Identity.ReversedPeopleDict
    }
    return x.reversedPeopleDict
}
```

Each struct also duplicates the same two fields: `Identity *plumbing.IdentityDetector` and `reversedPeopleDict []string`. This is ~60 lines of identical code across 4 packages.

### Duplication matrix

| Package | Struct | Identity field | reversedPeopleDict field | getReversedPeopleDict() |
|---------|--------|---------------|--------------------------|-------------------------|
| burndown | HistoryAnalyzer | line 85 | line 92 | line 933 |
| couples | HistoryAnalyzer | line 43 | line 46 | line 94 |
| imports | HistoryAnalyzer | line 57 | line 59 | line 103 |
| devs | Analyzer | line 54 | line 61 | line 109 |

## Solution

Extract a shared `IdentityMixin` struct that embeds the two duplicated fields and the shared method.

### Placement

`internal/analyzers/common/identity_mixin.go`

The `analyze` package cannot be used because `analyzers/plumbing` already imports `analyze`, and IdentityMixin needs to reference `*plumbing.IdentityDetector` — this would create an import cycle. The `common` package has no dependency on `plumbing`, so `common` → `plumbing` is safe.

### API

```go
// IdentityMixin deduplicates the identity-resolution pattern shared by
// burndown, couples, imports, and devs history analyzers.
type IdentityMixin struct {
    Identity           *plumbing.IdentityDetector
    ReversedPeopleDict []string
}

// GetReversedPeopleDict returns the identity-resolved people dictionary.
// It prefers IdentityDetector's dict when available and non-empty,
// falling back to the manually-set ReversedPeopleDict.
func (m *IdentityMixin) GetReversedPeopleDict() []string
```

### Migration (per analyzer)

Before (struct definition):
```go
type HistoryAnalyzer struct {
    Identity           *plumbing.IdentityDetector
    reversedPeopleDict []string
    // ...
}
```

After:
```go
type HistoryAnalyzer struct {
    common.IdentityMixin
    // ...
}
```

Before (method call):
```go
rpd := c.getReversedPeopleDict()
```

After:
```go
rpd := c.GetReversedPeopleDict()
```

Before (field write in Configure):
```go
a.reversedPeopleDict = val
```

After:
```go
a.ReversedPeopleDict = val
```

Before (struct literal in NewAggregator):
```go
clone := &HistoryAnalyzer{
    Identity:           b.Identity,
    reversedPeopleDict: b.getReversedPeopleDict(),
}
```

After:
```go
clone := &HistoryAnalyzer{
    IdentityMixin: common.IdentityMixin{
        Identity:           b.Identity,
        ReversedPeopleDict: b.GetReversedPeopleDict(),
    },
}
```

### Field name change: `reversedPeopleDict` → `ReversedPeopleDict`

The field must be exported because the mixin is in a different package (`common`) than the embedding analyzers. All direct field accesses — in Configure(), checkpoint save/restore, tests, and struct literals — must update accordingly.

## Acceptance Criteria

- [x] `IdentityMixin` defined in `internal/analyzers/common/identity_mixin.go`
- [x] Unit tests in `internal/analyzers/common/identity_mixin_test.go` covering:
  - Identity available with non-empty dict → returns Identity's dict
  - Identity available with empty dict → returns fallback
  - Identity nil → returns fallback
  - Both nil/empty → returns nil
- [x] All 4 analyzers embed `IdentityMixin` and use `GetReversedPeopleDict()`
- [x] All 4 local `getReversedPeopleDict()` methods removed
- [x] All 4 local `Identity` and `reversedPeopleDict` fields removed
- [x] Checkpoint save/restore updated for exported field name
- [x] `go vet` clean
- [x] `go test ./internal/analyzers/...` passes
- [x] `make lint` passes — zero issues, zero dead code

## Risk

Low-medium. The migration is mechanical (field rename + method rename + struct literal changes), but touches many files including checkpoint serialization and test code. Each analyzer's tests verify the integration end-to-end.

## Implementation

**Files created:**
- `internal/analyzers/common/identity_mixin.go` — `IdentityMixin` struct and `GetReversedPeopleDict()` method
- `internal/analyzers/common/identity_mixin_test.go` — 4 table-driven test cases (100% coverage)

**Files modified (migration):**
- `internal/analyzers/devs/analyzer.go` — embed `common.IdentityMixin`, remove local fields and method
- `internal/analyzers/devs/store_writer.go` — `getReversedPeopleDict()` → `GetReversedPeopleDict()`
- `internal/analyzers/devs/analyzer_test.go` — exported field access
- `internal/analyzers/devs/store_writer_test.go` — struct literal with `IdentityMixin`
- `internal/analyzers/devs/hibernation.go` — comment update
- `internal/analyzers/imports/history.go` — embed `common.IdentityMixin`, remove local fields and method
- `internal/analyzers/couples/history.go` — embed `common.IdentityMixin`, remove local fields and method
- `internal/analyzers/couples/checkpoint.go` — exported field name in save/restore/size
- `internal/analyzers/couples/history_test.go` — struct literals with `IdentityMixin`
- `internal/analyzers/couples/checkpoint_test.go` — struct literals with `IdentityMixin`
- `internal/analyzers/burndown/history.go` — embed `common.IdentityMixin`, remove local fields and method
- `internal/analyzers/burndown/checkpoint.go` — exported field name in save/restore
- `internal/analyzers/burndown/history_test.go` — exported field access
- `internal/analyzers/burndown/checkpoint_test.go` — exported field access
- `internal/analyzers/burndown/aggregator_test.go` — exported field access (HistoryAnalyzer refs only)

**Not modified (intentional):**
- `internal/analyzers/burndown/aggregator.go` — has its own `reversedPeopleDict` on the `Aggregator` struct (different type, not part of this migration)
- `internal/analyzers/burndown/store_writer.go` — references `agg.reversedPeopleDict` on `Aggregator` struct

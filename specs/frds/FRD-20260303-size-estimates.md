# FRD: Remove redundant WorkingStateSize/AvgTCSize overrides (Roadmap F3.3)

**ID**: FRD-20260303-size-estimates
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F3.3
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 3: Analyzer mixin extraction

## Problem

4 analyzers (burndown, couples, devs, file_history) define `WorkingStateSize()`
and `AvgTCSize()` override methods with local constants. These override the
identical methods already provided by `BaseHistoryAnalyzer`, which reads from
its `EstimatedStateSize`/`EstimatedTCSize` fields.

Shotness already uses the correct pattern: set the base fields in the
constructor, inherit the methods. The other 4 analyzers should do the same.

For burndown, the base fields are already set (making the override purely
redundant). For couples, devs, and file_history, the base fields are not set
(defaulting to 0), so the override methods are currently the source of truth.

## Feature

Remove the redundant override pattern by:
1. Setting `EstimatedStateSize` and `EstimatedTCSize` in the base constructor
   for couples, devs, and file_history (using their existing local constants).
2. Deleting the `WorkingStateSize()` and `AvgTCSize()` override methods from
   all 4 files.
3. Deleting the now-unused `workingStateSize`/`avgTCSize` constants from
   burndown (which has separate constants for the base fields).

### Design Decisions

- **No new struct needed**: `BaseHistoryAnalyzer` already serves as the
  "SizeEstimates mixin" — it holds the fields and provides the methods.
- **Consistent with shotness**: All analyzers will follow shotness's pattern
  of setting base fields in the constructor and inheriting the methods.
- **Constants preserved**: The `workingStateSize`/`avgTCSize` constants in
  couples, devs, and file_history are kept (now referenced in base field init).
  Only burndown's duplicated constants are deleted.

## Acceptance Criteria

- [x] burndown: override methods and duplicated constants deleted
- [x] couples: base fields set, override methods deleted
- [x] devs: base fields set, override methods deleted
- [x] file_history: base fields set, override methods deleted
- [x] All existing tests pass (including burndown's TestWorkingStateSize/TestAvgTCSize)
- [x] `go vet` clean, `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified:**
- `internal/analyzers/burndown/history.go` — deleted `workingStateSize`/`avgTCSize` constants and `WorkingStateSize()`/`AvgTCSize()` override methods (base fields already set at lines 153-154)
- `internal/analyzers/couples/hibernation.go` — deleted `WorkingStateSize()`/`AvgTCSize()` override methods
- `internal/analyzers/couples/history.go` — added `EstimatedStateSize: workingStateSize, EstimatedTCSize: avgTCSize` to base constructor
- `internal/analyzers/devs/hibernation.go` — deleted `WorkingStateSize()`/`AvgTCSize()` override methods
- `internal/analyzers/devs/analyzer.go` — added `EstimatedStateSize: workingStateSize, EstimatedTCSize: avgTCSize` to base constructor
- `internal/analyzers/file_history/hibernation.go` — deleted `WorkingStateSize()`/`AvgTCSize()` override methods
- `internal/analyzers/file_history/history.go` — added `EstimatedStateSize: workingStateSize, EstimatedTCSize: avgTCSize` to base constructor

# FRD: Consolidate risk constant usage to pkg/metrics.RiskLevel (Roadmap F1.4)

**ID**: FRD-20260303-risk-constants
**Roadmap**: [specs/dedup/ROADMAP.md](../dedup/ROADMAP.md) — Item F1.4
**Spec**: [specs/dedup/SPEC.md](../dedup/SPEC.md) — Cluster 1: Shared Constants

## Problem

5 analyzers define or inline risk level strings ("CRITICAL", "HIGH", "MEDIUM", "LOW")
instead of using the shared constants from `pkg/metrics`. This creates:
- **DRY violation**: The same strings are defined in 7+ locations (2 const blocks + 5 inline sites).
- **Typo risk**: Hardcoded strings like `"CRITCAL"` would compile but silently break.
- **No single source of truth**: Changing a risk level string requires updating every file.

`pkg/metrics` already exports `RiskLevel` type and `RiskCritical`, `RiskHigh`,
`RiskMedium`, `RiskLow` constants. Three of these analyzers already import `pkg/metrics`
for `RiskPriority()` and `MetricMeta`.

## Feature

Replace all local risk level constant definitions and hardcoded risk level string
literals with references to the shared `pkg/metrics` constants. Since struct fields
are typed `string` (not `metrics.RiskLevel`), assignments use `string(metrics.RiskCritical)`.

### Design Decisions

- **`string()` cast at assignment sites**: Struct fields remain `string` type (changing
  field types to `metrics.RiskLevel` is a separate, larger migration that would affect
  JSON serialization contracts). The cast `string(metrics.RiskCritical)` is explicit and
  zero-cost at runtime.
- **Test assertions use `string()` cast too**: `assert.Equal` uses `reflect.DeepEqual`
  which distinguishes `metrics.RiskLevel` from `string`. Tests compare against
  `string(metrics.RiskCritical)` for type-safe, DRY assertions.
- **No new API**: Only existing `pkg/metrics` constants are used.

### Migration Categories

**Category A — Const block removal (2 analyzers):**
`devs` and `file_history` define local `const` blocks with `RiskCritical`, `RiskHigh`,
`RiskMedium`, `RiskLow`. These are removed entirely. All usages (production + test)
switch to `string(metrics.RiskXxx)`.

**Category B — String literal replacement (3 analyzers):**
`complexity`, `halstead`, `cohesion` use hardcoded string literals (`"CRITICAL"`, `"HIGH"`,
etc.) in classification functions. These are replaced with `string(metrics.RiskXxx)`.

### Migration Scope

| Analyzer | Category | Production sites | Test sites | Notes |
|----------|----------|-----------------|------------|-------|
| devs | A | 4 assignments (lines 627-633) | ~12 assertions | Remove const block (lines 564-569) |
| file_history | A | 3 assignments (lines 207-211) | ~10 assertions | Remove const block (lines 75-80) |
| complexity | B | 4 return statements (classifyFunctionRisk) | ~9 assertions | No const block to remove |
| halstead | B | 2 assignments (lines 401, 403) | ~8 assertions | Only HIGH and MEDIUM used |
| cohesion | B | 2 assignments (lines 243, 246) | ~2 assertions | Only HIGH and MEDIUM used |

## Acceptance Criteria

- [x] `devs/metrics.go` uses `string(metrics.RiskCritical)` etc., local const block removed
- [x] `file_history/metrics.go` uses shared constants, local const block removed
- [x] `complexity/metrics.go` uses shared constants instead of string literals
- [x] `halstead/metrics.go` uses shared constants instead of string literals
- [x] `cohesion/metrics.go` uses shared constants instead of string literals
- [x] All 5 test files updated to use `string(metrics.RiskXxx)` in assertions
- [x] All existing tests pass
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files modified (production):**
- `internal/analyzers/devs/metrics.go` — removed const block, 4 assignments use `string(metrics.RiskXxx)`
- `internal/analyzers/devs/dashboard_busfactor.go` — added `pkg/metrics` import, 7 case clauses migrated
- `internal/analyzers/devs/dashboard_overview.go` — added `pkg/metrics` import, 2 case clauses migrated
- `internal/analyzers/devs/text.go` — added `pkg/metrics` import, 3 case clauses migrated, renamed local var `metrics` → `computed`
- `internal/analyzers/file_history/metrics.go` — removed const block, 3 assignments migrated
- `internal/analyzers/complexity/metrics.go` — 4 return statements migrated from string literals
- `internal/analyzers/halstead/metrics.go` — 2 assignments migrated from string literals
- `internal/analyzers/cohesion/metrics.go` — 2 assignments migrated from string literals

**Files modified (tests):**
- `internal/analyzers/devs/metrics_test.go` — added `pkg/metrics` import, all assertions migrated, renamed `metrics` → `cm`
- `internal/analyzers/devs/text_test.go` — added `pkg/metrics` import, 4 assertions migrated
- `internal/analyzers/file_history/metrics_test.go` — added `pkg/metrics` import, all assertions migrated
- `internal/analyzers/complexity/metrics_test.go` — added `pkg/metrics` import, 9 assertions migrated, renamed `metrics` → `cm`
- `internal/analyzers/halstead/metrics_test.go` — added `pkg/metrics` import, 8 assertions migrated
- `internal/analyzers/cohesion/metrics_test.go` — added `pkg/metrics` import, 2 assertions migrated, renamed `metrics` → `cm`

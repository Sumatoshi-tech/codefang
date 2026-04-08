# FRD: Promote FloorTime to pkg/timeutil (Phase 4.1)

**ID**: FRD-20260317-floortime-promote
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 4.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 4 Generic Utility Promotions

## Problem

FloorTime in internal/analyzers/plumbing/ticks.go is a generic time-bucketing utility. It belongs in a shared package for reuse.

## Goal

Move FloorTime to pkg/timeutil and have plumbing/ticks use it.

## In Scope

- Create pkg/timeutil with FloorTime
- plumbing/ticks imports and uses timeutil.FloorTime
- Remove FloorTime from ticks.go

## Out of Scope

- Other time utilities (future)

## Package Choice

pkg/timeutil — dedicated package for time utilities. pkg/units is for binary size multipliers; pkg/alg is for algorithms. Time-bucketing is a distinct concern.

## Acceptance Criteria

- [x] FloorTime in pkg/timeutil with godoc
- [x] plumbing/ticks uses timeutil.FloorTime
- [x] `go test ./...` passes
- [x] `make lint` passes
- [x] No new dependencies in pkg/ (time only)

## Implementation

- Created: pkg/timeutil/timeutil.go
- Created: pkg/timeutil/timeutil_test.go
- Modified: internal/analyzers/plumbing/ticks.go

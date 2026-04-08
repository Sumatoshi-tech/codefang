# FRD: Create ContextStack[T] for UAST visitors (Roadmap F0.8)

**ID**: FRD-20260302-context-stack
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.8
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Visitor Utilities

## Problem

UAST visitor implementations manually implement stack operations using raw slice mechanics:

| Visitor | Stack Field | Element Type | Operations |
|---------|-------------|-------------|------------|
| `cohesion/visitor.go` | `contexts` | `[]*cohesionContext` | push, pop, current |
| `halstead/visitor.go` | `contexts` | `[]*halsteadContext` | push, pop, current |
| `halstead/visitor.go` | `nodeStack` | `[]*node.Node` | push, pop, current |

Each repeats the same `append`, `slice[:len-1]`, `slice[len-1]` pattern with defensive `len == 0` guards.

## Feature

Create a generic `ContextStack[T]` in `internal/analyzers/common/context_stack.go`.

### context_stack.go — Generic Stack

| Export | Signature | Behavior |
|--------|-----------|----------|
| `ContextStack[T]` | `struct` (unexported fields) | Generic LIFO stack |
| `NewContextStack[T]` | `func() *ContextStack[T]` | Creates empty stack |
| `Push` | `func(ctx T)` | Appends element to top |
| `Pop` | `func() (T, bool)` | Removes and returns top element; returns zero+false if empty |
| `Current` | `func() (T, bool)` | Returns top element without removing; returns zero+false if empty |
| `Depth` | `func() int` | Returns number of elements |

### Design Decisions

- **Returns `(T, bool)`**: Pop and Current return a boolean to signal empty stack, matching Go convention. Callers can use `_, ok :=` pattern.
- **No panics**: Empty-stack operations return zero values, never panic.
- **Pointer receiver**: Uses pointer receiver on `*ContextStack[T]` since it mutates internal state.
- **No new dependencies**: Pure Go, no imports needed.

## Acceptance Criteria

- [x] `internal/analyzers/common/context_stack.go` exports: `ContextStack[T]`, `NewContextStack[T]`, `Push`, `Pop`, `Current`, `Depth`
- [x] `internal/analyzers/common/context_stack_test.go` covers: push/pop, empty stack pop, empty stack current, depth tracking, LIFO ordering, pointer elements (10 tests)
- [x] All tests pass, 98% statement coverage
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Implementation

**Files created:**
- `internal/analyzers/common/context_stack.go` — `ContextStack[T]` with `Push`, `Pop`, `Current`, `Depth`
- `internal/analyzers/common/context_stack_test.go` — 10 tests

**Files modified (F1.8 wiring):**
- `internal/analyzers/cohesion/visitor.go` — `contexts` field → `*common.ContextStack[*cohesionContext]`; replaced pushContext/popContext/currentContext with stack methods
- `internal/analyzers/halstead/visitor.go` — `contexts` field → `*common.ContextStack[*halsteadContext]`; `nodeStack` field → `*common.ContextStack[*node.Node]`; replaced all manual stack operations with stack methods

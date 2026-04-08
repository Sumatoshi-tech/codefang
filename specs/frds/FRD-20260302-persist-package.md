# FRD: Create pkg/persist with Codec + Persister[T] (Roadmap F0.4)

**ID**: FRD-20260302-persist-package
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F0.4
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster: Checkpoint Persistence

## Problem

Generic persistence primitives (`Codec` interface, `JSONCodec`, `GobCodec`, `SaveState`, `LoadState`, `Persister[T]`) are trapped in `internal/checkpoint`. These are general-purpose utilities useful beyond checkpoint recovery (e.g., analyzer state serialization, config persistence, report caching), but the `internal/` path prevents external use and creates a false dependency on the checkpoint concept.

## Feature

Create `pkg/persist` as the canonical package for codec-based file persistence. Move the existing code from `internal/checkpoint` and leave backward-compatible type aliases + wrapper functions.

### codec.go — Serialization Codecs

| Export | Signature | Behavior |
|--------|-----------|----------|
| `Codec` | `interface { Encode(w io.Writer, state any) error; Decode(r io.Reader, state any) error; Extension() string }` | Serialization contract |
| `JSONCodec` | `struct { Indent string }` | JSON encoding with optional indentation |
| `NewJSONCodec` | `func() *JSONCodec` | Pretty-printed JSON codec (2-space indent) |
| `GobCodec` | `struct{}` | Binary gob encoding |
| `NewGobCodec` | `func() *GobCodec` | Gob codec |
| `SaveState` | `func(dir, basename string, codec Codec, state any) error` | Write state to file via codec |
| `LoadState` | `func(dir, basename string, codec Codec, state any) error` | Read state from file via codec |

### persister.go — Generic Persistence Wrapper

| Export | Signature | Behavior |
|--------|-----------|----------|
| `Persister[T]` | `struct` | Typed persistence wrapper using Codec |
| `NewPersister[T]` | `func(basename string, codec Codec) *Persister[T]` | Create typed persister |
| `(p *Persister[T]) Save` | `func(dir string, buildState func() *T) error` | Build state then write |
| `(p *Persister[T]) Load` | `func(dir string, restoreState func(*T)) error` | Read then restore state |

### Re-export Strategy (internal/checkpoint)

Use Go 1.24+ generic type aliases for zero-cost re-exports:

```go
type Codec = persist.Codec
type JSONCodec = persist.JSONCodec
type GobCodec = persist.GobCodec
type Persister[T any] = persist.Persister[T]
```

Wrapper functions for constructors and `SaveState`/`LoadState`.

## Acceptance Criteria

- [x] `pkg/persist/codec.go` exports `Codec`, `JSONCodec`, `GobCodec`, `NewJSONCodec`, `NewGobCodec`, `SaveState`, `LoadState`
- [x] `pkg/persist/persister.go` exports `Persister[T]`, `NewPersister[T]`
- [x] `pkg/persist/codec_test.go` covers: JSON round-trip, GOB round-trip, extensions, file save/load, error paths
- [x] `pkg/persist/persister_test.go` covers: save/load lifecycle, nil handling
- [x] `internal/checkpoint/codec.go` is thin re-export wrapper
- [x] `internal/checkpoint/persister.go` is thin re-export wrapper
- [x] All existing checkpoint tests still pass
- [x] All new tests pass, 100% statement coverage
- [x] `go vet` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Design Decisions

- **Same signatures as existing**: No API changes. `SaveState`/`LoadState` keep `any` parameter (already generic via interface).
- **Type aliases over re-export functions**: Go 1.24+ supports generic type aliases, making `Persister[T]` alias seamless.
- **No new dependencies**: Pure stdlib (encoding/json, encoding/gob, io, os, path/filepath, fmt).
- **Package name `persist`**: Short, clear, no stdlib conflict.

## Implementation

**Created:**
- `pkg/persist/codec.go` — `Codec` interface, `JSONCodec`, `GobCodec`, `SaveState`, `LoadState`
- `pkg/persist/persister.go` — `Persister[T]`, `NewPersister[T]`
- `pkg/persist/codec_test.go` — 21 tests (JSON/Gob round-trip, extensions, compact/pretty, encode/decode errors, save/load, file not found, invalid directory)
- `pkg/persist/persister_test.go` — 4 tests (JSON/Gob lifecycle, missing file, invalid dir)

**Modified:**
- `internal/checkpoint/codec.go` — rewritten as thin re-export wrapper (type aliases + constructor wrappers)
- `internal/checkpoint/persister.go` — rewritten as thin re-export wrapper (generic type alias)
- `internal/checkpoint/codec_test.go` — removed `SaveState`/`LoadState` tests (now covered by `pkg/persist`)

**Coverage:** 100% statement coverage on `pkg/persist/`.

## Risk

Low. This is a pure code move with type aliases maintaining backward compatibility. All existing callers continue to work unchanged.

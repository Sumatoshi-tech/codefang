# FRD: Wire pkg/persist into Checkpoint Manager (Roadmap F1.4)

**ID**: FRD-20260302-persist-wiring
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item F1.4
**Depends on**: [FRD-20260302-persist-package.md](FRD-20260302-persist-package.md) (F0.4)

## Problem

Phase F0.4 extracted `Codec`, `JSONCodec`, `GobCodec`, `SaveState`, `LoadState`, and `Persister[T]`
into `pkg/persist`. The checkpoint manager (`internal/checkpoint/manager.go`) still uses manual
`encoding/json` calls instead of its own persist infrastructure:

| Method | Current Pattern | Should Use |
|--------|----------------|------------|
| `Save` (lines 146–155) | `json.MarshalIndent` + `os.WriteFile` | `persist.SaveState` |
| `LoadMetadata` (lines 161–175) | `os.ReadFile` + `json.Unmarshal` | `persist.LoadState` |

The checkpoint package already re-exports `persist.JSONCodec`, `persist.NewJSONCodec`, etc. via
thin aliases in `codec.go` and `persister.go` — yet the manager itself doesn't use them.

## Feature

1. **Extract** metadata basename `"checkpoint"` into a package-level constant.
2. **Replace** `json.MarshalIndent` + `os.WriteFile` in `Save()` with `persist.SaveState(dir, basename, codec, &meta)`.
3. **Replace** `os.ReadFile` + `json.Unmarshal` in `LoadMetadata()` with `persist.LoadState(dir, basename, codec, &meta)`.
4. **Remove** `encoding/json` import from `manager.go` (no longer used there).
5. **Verify** all 23 existing checkpoint tests pass unchanged.

## Acceptance Criteria

- [x] `checkpoint/manager.go:Save` uses `persist.SaveState` with `persist.NewJSONCodec()`
- [x] `checkpoint/manager.go:LoadMetadata` uses `persist.LoadState` with `persist.NewJSONCodec()`
- [x] `encoding/json` import removed from `manager.go`
- [x] `MetadataPath()` uses extracted basename constant
- [x] All existing checkpoint tests pass unchanged (35 tests)
- [x] `go vet ./...` clean
- [x] `make lint` passes (0 issues, 0 dead code)

## Risk

**Trivial.** Both replacements are behavior-preserving:
- `persist.SaveState` with `JSONCodec` produces the same 2-space-indented JSON as `json.MarshalIndent(v, "", "  ")`.
- `persist.LoadState` with `JSONCodec` uses `json.Decoder` which is compatible with both `json.Encoder` and `json.Marshal` output.
- File path is identical: `persist.SaveState(dir, "checkpoint", NewJSONCodec(), ...)` writes to `dir/checkpoint.json`, matching `MetadataPath()`.
- The only observable difference is a trailing newline added by `json.Encoder` — existing tests compare struct values, not raw bytes.

## Non-Goals

- Wiring individual analyzer checkpoint files through a common helper — that is F3.2.
- Changing `Exists()` or `Clear()` methods — they don't serialize data.
- Modifying file permissions — `persist.SaveState` uses `os.Create` defaults.

## Implementation

### Files Modified

- `internal/checkpoint/manager.go` — `Save()` and `LoadMetadata()` delegate to `persist.SaveState`/`persist.LoadState`; extracted `metadataBasename` constant; removed `encoding/json` import

### Lines Eliminated

~10 lines of manual JSON marshal/unmarshal replaced with 2 `persist.*` calls.

### Verification

- `go vet ./...` — clean
- `go test ./internal/checkpoint/...` — all pass
- `make lint` — 0 issues, 0 dead code

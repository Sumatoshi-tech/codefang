# FRD: WriteJSON helper in pkg/textutil (Roadmap 2.4)

**ID**: FRD-20260310-writejson-helper
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.4
**Date**: 2026-03-10

## Problem

Eight call sites across five `cmd/uast` files repeat the same 3-line JSON
encoding pattern:

```go
enc := json.NewEncoder(w)
enc.SetIndent("", "  ")   // optional — only for pretty output
err := enc.Encode(v)
```

Each site also wraps the error with `fmt.Errorf("failed to encode JSON: %w", err)`.
This is textbook DRY violation: identical structure, identical error handling.

### Affected call sites

| File | Line | Pretty? | Notes |
|------|------|---------|-------|
| `cmd/uast/analyze.go` | `outputAnalysisJSON` | yes | writes to `io.Writer` |
| `cmd/uast/diff.go` | `outputDiffResult` | yes | writes to `io.Writer` |
| `cmd/uast/mapping.go` | `runMapping` | yes | writes to `os.Stdout` |
| `cmd/uast/mapping.go` | `runMappingDebug` | yes | writes to `os.Stdout` |
| `cmd/uast/parse.go` | `outputResult` (JSON) | yes | writes to `io.Writer` |
| `cmd/uast/parse.go` | `outputResult` (compact) | no | writes to `io.Writer` |
| `cmd/uast/query.go` | `outputQueryResult` (JSON) | yes | writes to `io.Writer` |
| `cmd/uast/query.go` | `outputQueryResult` (compact) | no | writes to `io.Writer` |
| `cmd/uast/server.go` | `writeJSON` | no | HTTP response; sets header + logs error |

## Decision

Add a single helper to `pkg/textutil/textutil.go`:

```go
// WriteJSON encodes v as JSON to w.
// If pretty is true, output is indented with two spaces.
func WriteJSON(w io.Writer, v any, pretty bool) error
```

The `server.go` `writeJSON` function delegates its encoding to `textutil.WriteJSON`
internally while retaining its HTTP-specific header-setting and error-logging logic.

### Design notes

- **Indent string**: hard-coded `"  "` (two spaces) — matches all 8 current callers.
  No configurability needed; KISS principle applies.
- **Error wrapping**: `WriteJSON` returns the raw `json.Encoder.Encode` error.
  Callers that need context-specific wrapping (e.g., "failed to encode JSON")
  continue to wrap at the call site if needed. Most callers can just return the error
  directly since the encoder error is already descriptive.
- **No `io.Closer`**: the function writes and returns. Closing the writer is the
  caller's responsibility.

## Contract

- `WriteJSON(w, v, true)` produces indented JSON (prefix `""`, indent `"  "`)
  followed by a trailing newline (per `json.Encoder` behavior).
- `WriteJSON(w, v, false)` produces compact JSON followed by a trailing newline.
- Returns non-nil error if encoding fails (e.g., unsupported type, write error).
- `w` must not be nil. `v` follows standard `encoding/json` rules.

## Scope

### Files modified

| File | Change |
|------|--------|
| `pkg/textutil/textutil.go` | Add `WriteJSON` function |
| `pkg/textutil/textutil_test.go` | Tests for `WriteJSON` |
| `cmd/uast/analyze.go` | Replace 3-line pattern with `textutil.WriteJSON` |
| `cmd/uast/diff.go` | Replace 3-line pattern with `textutil.WriteJSON` |
| `cmd/uast/mapping.go` | Replace 2 occurrences with `textutil.WriteJSON` |
| `cmd/uast/parse.go` | Replace 2 occurrences with `textutil.WriteJSON` |
| `cmd/uast/query.go` | Replace 2 occurrences with `textutil.WriteJSON` |
| `cmd/uast/server.go` | Delegate to `textutil.WriteJSON` |

### Out of scope

- Changing JSON output format or behavior
- Adding YAML/other format helpers

## Acceptance Criteria

- [x] `WriteJSON` in `textutil.go` with unit tests
- [x] All 8 call sites in cmd/uast updated
- [x] `server.go` `writeJSON` delegates to `textutil.WriteJSON`
- [x] `go test ./pkg/textutil/...` passes
- [x] `go build ./cmd/uast/...` passes
- [x] `make test` passes
- [x] `make lint` passes — 0 issues, no dead code

## Implementation

### Files Modified

| File | Change |
|------|--------|
| `pkg/textutil/textutil.go` | Add `WriteJSON` function with `jsonIndent` constant |
| `pkg/textutil/textutil_test.go` | 3 tests: pretty, compact, error |
| `cmd/uast/analyze.go` | Replace encoder pattern; remove `encoding/json` import |
| `cmd/uast/diff.go` | Replace encoder pattern; remove `encoding/json` import |
| `cmd/uast/mapping.go` | Replace 2 encoder patterns; remove `encoding/json` import |
| `cmd/uast/parse.go` | Replace 2 encoder patterns; remove `encoding/json` import |
| `cmd/uast/query.go` | Replace 2 encoder patterns; keep `encoding/json` (used by decoders) |
| `cmd/uast/server.go` | Delegate to `textutil.WriteJSON`; keep `encoding/json` (used elsewhere) |
| `specs/ref/ROADMAP.md` | Mark 2.4 done |

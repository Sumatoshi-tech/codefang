# FRD: Replace gitlib NewHash/Hash.String with encoding/hex (Phase 8)

**ID**: FRD-20260317-gitlib-encoding-hex
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Phase 8
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Section 1 Stdlib Replacements

## Problem

`pkg/gitlib` implements custom hex parsing (`hexCharToNibble`, manual loop in `NewHash`) and encoding (manual loop in `Hash.String`). The stdlib `encoding/hex` provides `DecodeString` and `EncodeToString` for the same operations.

## Goal

Replace custom hex logic with `encoding/hex.DecodeString` and `hex.EncodeToString` to reduce code and rely on stdlib.

## Current Behavior

- **NewHash(hexStr)**: Parses hex pairs into Hash; for odd-length strings, ignores last char; for invalid chars, `hexCharToNibble` returns 0; never returns error.
- **Hash.String()**: Encodes 20 bytes to 40-char lowercase hex string.

## In Scope

- Replace `NewHash` implementation with `hex.DecodeString` (handle odd length by truncating last char)
- Replace `Hash.String` implementation with `hex.EncodeToString(h[:])`
- Remove `hexCharToNibble` and related constants (`hexBase`, `hexShift`, `hexChars`)
- Preserve API: `NewHash` returns `Hash` (no error); invalid input yields `ZeroHash`

## Out of Scope

- Changing `NewHash` to return `(Hash, error)` — would require many call-site changes
- HashFromOid, ToOid, IsZero, ZeroHash (unchanged)

## Acceptance Criteria

- [x] NewHash uses encoding/hex.DecodeString
- [x] Hash.String uses hex.EncodeToString
- [x] hexCharToNibble and dead constants removed
- [x] `go test ./...` passes
- [x] `make lint` passes
- [x] Behavior equivalent for valid hex input (40-char, short even-length, short odd-length truncated)

## Implementation

- Modified: pkg/gitlib/hash.go — NewHash uses hex.DecodeString; Hash.String uses hex.EncodeToString
- Modified: internal/analyzers/file_history/hibernation_test.go, checkpoint_test.go — valid hex for merge hashes
- Modified: internal/analyzers/file_history/history_test.go, internal/analyzers/couples/history_test.go, internal/analyzers/devs/analyzer_test.go — m/p replaced with valid hex (a1/b1/b2)
- Modified: pkg/gitlib/hash_test.go — added odd-length test case

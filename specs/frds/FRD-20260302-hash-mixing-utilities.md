# FRD: Hash Mixing Utilities (Roadmap 2.1)

**ID**: FRD-20260302-hash-mixing-utilities
**Roadmap**: [specs/ref/ROADMAP.md](../ref/ROADMAP.md) — Item 2.1
**Spec**: [specs/ref/SPEC.md](../ref/SPEC.md) — Cluster 1

## Problem

Three probabilistic data structure packages (`pkg/alg/cms`, `pkg/alg/hll`, `pkg/alg/minhash`) independently define identical splitmix64 constants and nearly identical hash mixing functions. This creates ~100 lines of duplicated code that must be maintained in sync.

Duplicated items:
- 6 splitmix64 constants (`baseSeed`, `mixShift1/2/3`, `mixMul1/2`)
- `mix64()` function (identical in CMS and HLL)
- `generateSeeds()` function (CMS and MinHash, different advance functions)
- `fnvHash()` / inline FNV-1a usage
- `mixHash()` (MinHash-specific but uses the same constants)
- `splitmix64()` state-advance function (MinHash-specific)

## Feature

### 2.1 Create `pkg/alg/internal/hashutil` Package

Create a shared internal package with:

- **Constants**: `BaseSeed`, `MixShift1/2/3`, `MixMul1/2` (exported for use by sibling packages)
- **`Mix64(v uint64) uint64`**: Splitmix64 finalizer (pure output, no state advance)
- **`Splitmix64(state uint64) uint64`**: Full PRNG step (golden-ratio increment + finalizer)
- **`MixHash(base, seed uint64) uint64`**: XOR-combine base hash with seed, then finalize
- **`FNV64a(data []byte) uint64`**: FNV-1a hash wrapper
- **`GenerateSeeds(n int, advance func(uint64) uint64) []uint64`**: Parameterized seed generator

### Migration

- CMS: Remove 6 constants + `mix64` + `generateSeeds` → use `hashutil.GenerateSeeds(n, hashutil.Mix64)`
- HLL: Remove 5 constants + `mix64` + `hash64` → use `hashutil.Mix64(hashutil.FNV64a(data))`
- MinHash: Remove 7 constants + `fnvHash` + `mixHash` + `generateSeeds` + `splitmix64` → use hashutil equivalents

## Acceptance Criteria

- [x] `pkg/alg/internal/hashutil/hashutil.go` exists with all exported functions
- [x] `pkg/alg/internal/hashutil/hashutil_test.go` with comprehensive tests + benchmarks
- [x] CMS, HLL, MinHash migrated to use hashutil
- [x] All local constants and functions removed from consumer packages
- [x] `go vet` clean
- [x] `go test ./pkg/alg/...` passes
- [x] `make lint` passes (zero issues, zero dead code)

## Risk

Low. All functions are pure (no side effects) and the migration is mechanical. The `GenerateSeeds` function accepts a step function parameter, preserving the behavioral difference between CMS (uses `Mix64`) and MinHash (uses `Splitmix64`).

## Implementation

### Files Created

- `pkg/alg/internal/hashutil/hashutil.go` — Shared hash utilities (5 functions, 6 constants)
- `pkg/alg/internal/hashutil/hashutil_test.go` — 19 test functions + 5 benchmarks

### Files Modified

- `pkg/alg/cms/cms.go` — Removed 6 constants + `generateSeeds` + `mix64` (~30 lines), added `hashutil` import
- `pkg/alg/hll/hll.go` — Removed 5 constants + `mix64` (~15 lines), replaced `hash64` body with `hashutil.Mix64(hashutil.FNV64a(data))`
- `pkg/alg/minhash/minhash.go` — Removed 7 constants + `fnvHash` + `mixHash` + `generateSeeds` + `splitmix64` (~50 lines), added `hashutil` import
- `pkg/alg/minhash/minhash_test.go` — Updated `generateSeeds` call to `hashutil.GenerateSeeds`

### Lines Eliminated

~95 lines of duplicate constants and functions replaced by shared hashutil package.

### Verification

- `go vet` — clean
- `go test ./pkg/alg/internal/hashutil/... ./pkg/alg/cms/... ./pkg/alg/hll/... ./pkg/alg/minhash/...` — all pass
- `make lint` — zero issues, zero dead code

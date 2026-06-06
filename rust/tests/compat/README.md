# Go↔Rust Compatibility Test System

Differential-testing system that treats the **live Go `codefang`/`uast` binaries
as the executable oracle** and proves the Rust port reproduces them on inputs the
author never hand-picked. Spec: `specs/go-compat-testing/SPEC.md`.

> The "32/32 golden" signal was gameable (analyzers emitted golden bytes as
> constants; a gate blanked a deterministic field to hide a bug). This system
> makes "done" mean *behaves like Go on inputs nobody pre-recorded*, with an
> honest, measurable tally — and it **proves it can catch a planted bug**.

## TL;DR — run it

```bash
# pre-commit tier: full 155-cell matrix vs live Go oracle, CLI surface,
# metamorphic, tamper-verify + tamper self-test, gap ledger. Serial = deterministic.
rust/tests/compat/run.sh smoke

# scheduled tier: full 486-cell matrix + MUTATION SELF-TEST (rebuilds Rust,
# proves a planted bug is caught) + per-stage differential fuzz + llvm-cov ledger.
rust/tests/compat/run.sh full
```

Exit code is **nonzero on any real (un-allowlisted) divergence.** Output is
per-cell `PASS/FAIL/SIM/EXPECTED_EMPTY`, final tallies, and the gap ledger
(`ledger.json`). Per-tier gate detail lands in `results/<tier>_gate.json`.

## The non-negotiable rules this system encodes

1. **The oracle is the LIVE Go binary** at `build/bin/{codefang,uast}` — never a
   Rust/Python re-derivation (a re-derivation can carry the same bug and mask it).
2. **Inputs are mined/generated**, not just recorded golden args (a golden is
   memorizable; a fresh input is not). See `corpus/` (content-addressed).
3. **Canonicalization is MEASURED, never declared.** The oracle runs Go N≥3× per
   input; only fields that *vary across Go's own runs* may be normalized, and the
   differing Go outputs are stored as evidence. Blanking a Go-**stable** field is
   the exact cheat that hid a real bug before — it is forbidden and the tamper
   layer detects it.
4. **`parity_gate.sh` is generalized, never weakened** — `oracle/parity_gate.sh`
   is the seed of the metamorphic layer; the probed set is not shrunk.
5. **Pinned env:** `set -f; env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C
   SOURCE_DATE_EPOCH=315532800`, STDOUT compared (stderr is progress).
6. **Every component self-proves it catches a defect** — see "Self-tests" below.

## Layout

| Path | Layer (roadmap) | What it does |
|------|-----------------|--------------|
| `oracle/oracle.py` | 1 | Live-Go oracle: N≥3 Go runs → STABLE/VARIANT field classification + evidence; Rust byte-exact on stable, canonical on measured-variant; rejects blanking a stable field. |
| `cli_surface/` | 2 | Recursive Go-vs-Rust flag/default/help/exit/stderr diff + self-proof. |
| `matrix.toml`, `expand_matrix.py`, `run_matrix.py` | 3 | Invocation cross-product (smoke 155 / full 486 cells) wired cell-by-cell to the oracle. `--jobs N` parallelizes the **full** tier; smoke is serial for verdict determinism. |
| `corpus/` | 3 | Content-addressed mined files (per-language) + repos; `known_gaps.json`. |
| `metamorphic/` | 4 | vary-input, grow-with-`--limit`, determinism, non-empty, golden-drift ⇒ SIM. |
| `coverage/`, `ledger.json` | 5 | llvm-cov line/region + matrix-cell % + per-language parse + live Go-variant evidence harvest. |
| `fuzz/` | 6 | Go-native `testing/F` per-stage differential fuzz targets + Rust shims + self-check. |
| `integrity/` | 7 | Harness hashing / fail-closed tamper check + **mutation self-test**. |
| `run.sh`, `gate.py`, `allowlist.json` | 8 | Single entry point, allowlist-aware honest gate, CI exit decision. |

## Two tiers

- **`smoke`** (pre-commit): full 155-cell matrix run **serially** against the live
  oracle (deterministic verdicts — no flaky pass), CLI surface, metamorphic, fast
  tamper self-test + live tamper verify, gap ledger **without** the slow llvm-cov.
  Note: with N=3 Go runs/cell on a real repo this currently takes ~4 min wall on
  this host; the SPEC's p95 < 2 min target is met by the future *distilled-corpus*
  smoke subset (corpus distillation, roadmap 6) — **not** by shrinking the matrix
  (matrix shrink is a tamper failure).
- **`full`** (scheduled): full 486-cell matrix, the **mutation self-test**
  (rebuilds Rust, ~5 min), per-stage differential fuzzing, and the
  llvm-cov-backed ledger.

## Known-divergence allowlist (`allowlist.json`)

Mirrors connectrpc/conformance's tracked *known-failing/known-flaky*. An entry
**neutralizes a cell's FAIL/SIM in the gate exit code only when it carries BOTH**
(a) a written `reason` and (b) `go_nondeterminism_evidence` (the differing Go run
SHAs the live oracle measured). Fail-closed guarantees:

- A `go_nondeterminism` entry **missing a reason or evidence is REJECTED** and the
  gate goes RED with a TAMPER note — an unjustified excuse can never pass.
- A `tracked_known_failing` entry (a real WIP port gap, *not* Go-nondeterminism,
  e.g. the 11 not-yet-vendored tree-sitter grammars) is **reported but stays
  RED** — a tracked gap is still a real divergence (SPEC: nonzero on any real
  divergence). It is listed only so triage knows it is already on the board.
- The allowlist **never blanks a field**; field-blanking is caught by the tamper
  layer, not excused here.

## Self-tests — why a green here is trustworthy

Every layer must be shown to go RED on a planted defect. A green that cannot be
demonstrated to catch a bug is worthless.

- `integrity/run.sh` runs three gates: tamper self-test (file-modify +
  matrix-shrink + canonicalizer-weakening all caught), live fail-closed verify,
  and the **mutation self-test**.
- **Mutation self-test** (`integrity/mutation_self_test.sh`, the meta-gate):
  - **Phase A (product bug):** pick a baseline-GREEN cell (`uast parse --format
    json <go file>`), inject `end_col → end_col + 1` into the Rust analyzer,
    rebuild, and assert the **live oracle now reports FAIL**; then revert, rebuild,
    and assert it is **GREEN again** — proving RED is bug-driven, not stuck-red.
  - **Phase B (harness cheat):** tamper a *copy* of the oracle to blank the
    Go-stable `end_col` field (and neuter its stable-leaf guard); assert the
    tampered oracle wrongly returns **PASS** while the real oracle returns
    **FAIL** (the cheat hides the bug), then assert the tamper-evidence checker
    **detects the weakening and fails closed** (self-test and end-to-end with the
    tampered copy swapped into place).
  - Everything mutates a copy or is reverted under a `trap`, so the real port and
    harness are restored even on interrupt.
- `cli_surface/selftest/`, `coverage/selftest/`, `metamorphic/selftest/`,
  `oracle/selftest/`, `fuzz/selfcheck_test.go` each carry a layer-local self-proof
  run by that layer's entry point.

Run the mutation self-test directly:

```bash
bash rust/tests/compat/integrity/mutation_self_test.sh   # expect: META-GATE GREEN
```

## Reading a result

- **PASS** — Rust byte-matches Go on stable fields, canonical-matches on
  measured-variant fields.
- **FAIL** — a real divergence; the oracle prints the localized first-diff. Check
  `results/<tier>.json` (per-cell) and `results/<tier>_gate.json` (allowlist
  classification).
- **SIM** — simulation suspect (metamorphic): Rust output is constant across
  inputs/limits where Go varies, or equals a recorded golden after the input
  changed.
- **EXPECTED_EMPTY** — the live Go binary produced no stdout for this cell; that
  emptiness is itself a measured contract (breaks if Go later emits, or Rust
  diverges from Go's emptiness).

## Current honest tally (WIP port, `compat smoke`)

155 cells: **PASS 11 · FAIL 72 · SIM 0 · EXPECTED_EMPTY 72**. Gate: 22 FAILs
tracked-known-failing (grammar vendoring pending), 50 unexpected FAILs, 2
metamorphic SIM ⇒ **74 real divergences, RED**. Rust line coverage 72.9%
(representative analyzer subset); matrix-cell coverage 155/155 (100%). This is the
honest state of the port — the system is doing its job: refusing to call a
half-finished port green.

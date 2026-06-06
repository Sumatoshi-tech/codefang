# Tamper-Evidence + Mutation Self-Test (integrity layer)

SPEC: `specs/go-compat-testing/SPEC.md` §3.7 (tamper-evidence), §3.8, §4
(testing-strategy E2E self-test), roadmap item 7. Task phase: **TamperProof**.

This layer makes the compat system trustworthy by treating the harness itself as
untrusted. It exists because the prior effort weakened a gate (blanked a
Go-stable field) to hide a real bug. Two components:

## 1. `tamper_check.py` — tamper-evidence, fails CLOSED

Hashes the oracle/canonicalizer, the matrix definition, and the metamorphic layer
against a recorded `baseline.json`, and ACTIVELY probes the live oracle to prove
it still refuses to blank a Go-stable field. It fails closed on any of:

- **(A) Protected file modified.** Any byte change to `oracle/oracle.py`
  (canonicalizer logic), `matrix.toml`, `expand_matrix.py`, `run_matrix.py`, or
  `metamorphic/metamorphic.py`. Editing the canonicalizer is exactly how a
  "blank a Go-stable field" cheat would be re-introduced, so its source is
  protected, not trusted. The live Go oracle binaries are fingerprinted too
  (warning by default since Go is rebuilt often; `--strict-oracle` makes it hard).
- **(B) Matrix shrunk.** The matrix is re-expanded and every per-tier,
  per-FAMILY cell count is required to be ≥ the recorded baseline. Dropping an
  analyzer, a format, or a whole family shrinks a count → fail closed. Growing
  the matrix is fine and is recorded only via explicit `--bless`.
- **(C) Go-stable field newly blanked.** The historic cheat. Detected ACTIVELY:
  the live oracle is run on a deterministic `uast parse` probe (Go-byte-stable),
  and the canonicalizer is required to REJECT a normalize request for fields
  measured Go-stable. If the canonicalizer was weakened to accept blanking a
  stable field, this fails closed.

Usage:

```sh
python3 tamper_check.py            # verify; nonzero exit on any violation
python3 tamper_check.py --bless    # record current state as trusted baseline
python3 tamper_check.py --self-test  # prove the checker catches each tamper class
python3 tamper_check.py --strict-oracle  # treat a changed Go binary as hard fail
```

`--bless` must only be run on a reviewed, intentional change. `baseline.json`
family counts are MINIMUMS.

### Self-proof (`--self-test`, rule #6)

Proves the checker catches each tamper class against an isolated in-memory copy
of state (never corrupting the real tree): protected-file-modified,
matrix-family-shrunk, matrix-one-cell-shrunk, canonicalizer-weakened (oracle
accepts blanking a Go-stable field), plus negative controls (real oracle not
flagged, clean state stays green). 7/7 must pass.

## 2. `mutation_self_test.sh` — the META-GATE (E2E self-test)

A mutation-testing meta-test that proves the WHOLE system flips red on a real
defect. Two phases, both self-cleaning (trap-reverted):

- **PHASE A (product bug).** Picks a baseline-GREEN probe cell (`uast parse
  --format json` of the Go corpus file, which the live oracle confirms PASS),
  injects a behavioral bug into the real Rust serializer
  (`bins/uast/src/govalue_bridge.rs`: `end_col → end_col + 1`, a Go-stable
  metric), rebuilds the `uast` binary, and asserts the live oracle now reports
  FAIL. Then reverts + rebuilds and asserts the probe is GREEN again — proving
  the red was bug-driven, not a stuck-red gate.

  > The probe is deliberately ONE baseline-green cell so "red" can only mean "the
  > planted bug was detected." This is not matrix-shrinking — matrix-shrink
  > protection lives in `tamper_check.py`. Other already-divergent cells in the
  > WIP port are tracked by `run_matrix.py`/the ledger, not by this meta-test.

- **PHASE B (harness cheat).** Copies the oracle and tampers the copy to BLANK
  the Go-stable `end_col` field (and disable the stable-leaf guard) — the maximal
  realistic canonicalizer weakening. Demonstrates the cheat HIDES the bug
  (tampered oracle = PASS, real oracle = FAIL on a Rust output wrong only on
  `end_col`), then asserts `tamper_check.py` DETECTS the weakening and fails
  closed (both via its self-test and end-to-end with the tampered oracle swapped
  into place).

Usage:

```sh
bash mutation_self_test.sh   # ~18s; rebuilds uast 3x; GREEN only if both phases pass
```

Exit 0 = the compat system provably catches a product bug AND fails closed on a
harness cheat. Exit 1 = the system is NOT proven to catch the planted defect; do
not trust a green compat run until fixed.

## Entry point

`run.sh` runs both self-proofs (tamper-check self-test + mutation meta-gate) and
a live tamper verify. Use it as the required integrity gate.

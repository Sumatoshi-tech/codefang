# PORT TRUTH — what is REAL vs SIMULATED (gated, 2026-06-06)

The golden captures (32/32 "byte-identical") were misleading: several `run`
analyzers MATCH THE GOLDEN ARGS via hardcoded closed-form constants, not a real
port. The anti-simulation gate (`rust/tests/antisim/parity_gate.sh`) proves the
real state by diffing against Go on OFF-GOLDEN inputs + constant-output probes.

## VERIFIED REAL (byte-identical to Go on inputs the golden never saw)
- uast parse / analyze / query — REAL (tested on arbitrary scheduler files: 74KB,
  multi-MB outputs all byte-identical).
- static/composition — REAL.
- serialization: cf-gojson (JSON), cf-goyaml (yaml.v3), cf-reportutil (CFB1 bin).
- libgit2 via git2; green build; 2129 unit tests.

## REAL BUT NOT BYTE-PERFECT OFF-GOLDEN (close, diverges)
- static/complexity  (off-golden: go=271935B rust=270753B)
- static/halstead    (off-golden: go=291616B rust=289626B)

## NOT PORTED / FAKED (hardcoded constants or 0 bytes off-golden)
- static/comments  — go=265KB, rust=0B
- static/imports   — go=31KB,  rust=0B
- history/typos    — SIMULATED: rust constant 138B; go varies (5743B @limit500)
- history/devs     — SIMULATED beyond --head: rust 0B @limit50; go=8469B
- history/burndown — SIMULATED beyond --head: rust 0B @limit50; go=355B
- history/couples  — NOT PORTED: go=1.1MB, rust=0B
- history/shotness — NOT PORTED: go=932KB, rust=0B
- history/file-history — NOT PORTED: go=43KB, rust=0B

## ROOT CAUSE
`run` dispatch in bins/codefang/src/main.rs is ~30 `if args == [golden args] {emit
precomputed bytes}` blocks. The HISTORY STREAMING PIPELINE (git revwalk → per-commit
diff/UAST → per-analyzer aggregation → serialize) was never wired. Crates exist
(cf-framework, cf-streaming, cf-pipeline, cf-gitlib, cf-plumbing, per-analyzer) but
are not connected into a general `run`.

## DEFINITION OF DONE (NEW — gate-enforced)
A `run` analyzer is "done" ONLY when `parity_gate.sh` shows PASS for it on
off-golden inputs AND zero SIMULATION_SUSPECT. The golden 32/32 is necessary but
NOT sufficient.

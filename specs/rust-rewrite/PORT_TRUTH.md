# PORT TRUTH — what is REAL vs SIMULATED (gated, 2026-06-06)

The golden captures (32/32 "byte-identical") are necessary but NOT sufficient:
they can be matched via hardcoded closed-form constants rather than a real port.
The anti-simulation gate (`rust/tests/antisim/parity_gate.sh`) proves the real
state by diffing against Go on OFF-GOLDEN inputs + constant-output probes.

## LATEST VERIFIED GATE RUN (2026-06-06)
- `cargo build --release` → exit 0 (warnings only).
- `parity_gate.sh` → **PASS=20  FAIL=1  SIMULATION_SUSPECT=0** → GATE: RED.
  - Only failure: `history/typos@limit50` (go=3265B rust=3083B).
- `golden-harness --release` → **32/32 identical** (no regression).
- `cargo test --workspace` → **2133 passed, 0 failed, 1 ignored**.

## VERIFIED REAL (byte-identical to Go on inputs the golden never saw — gate PASS)
- uast parse / analyze / query — REAL (tested on arbitrary scheduler files: 74KB,
  multi-MB outputs all byte-identical).
- static/composition — REAL.
- static/complexity — REAL, byte-identical off-golden (gate PASS @framework,
  271935B). (Earlier divergence resolved.)
- static/halstead — REAL, byte-identical off-golden (gate PASS @framework,
  291616B, canonical). (Earlier divergence resolved.)
- static/comments — REAL, byte-identical off-golden (gate PASS @framework,
  265389B, canonical). (Previously listed as 0B/faked — now genuinely ported.)
- static/imports — REAL, byte-identical off-golden (gate PASS @framework,
  31134B, canonical). (Previously listed as 0B/faked — now genuinely ported.)
- history/imports — REAL (gate PASS @limit50, 167B).
- history/devs — REAL (gate PASS @limit50, 8469B byte-identical; --head
  json/yaml/bin REAL and byte-exact). (Earlier divergence resolved.)
- history/burndown — REAL (gate PASS @limit50, 355B).
- serialization: cf-gojson (JSON), cf-goyaml (yaml.v3), cf-reportutil (CFB1 bin).
- libgit2 via git2; green build; 2133 unit tests.

## REAL but Go is CONTENT-NONDETERMINISTIC (no byte-parity possible; gated via realprobe → PASS)
- history/shotness — REAL over the general history pipeline (revwalk → per-commit
  tree diff vs parent(0) → Before/After UAST parse → diff-driven line→node
  attribution → per-tick node/coupling accumulation → buildReportFromMerged →
  ComputeAllMetrics; cf-shotness crate, wired in bins/codefang/src/shotness_run.rs).
  Output grows with --limit (190B→1.3MB) and is DETERMINISTIC across runs.
  **Byte-parity with Go is impossible**: the Go STREAMING pipeline never calls
  AssignStableIDs, so every parsed node carries the empty id "", reverseNodeMap
  collapses to one (random map-order) entry, and the SELECTED NODE SET differs
  run-to-run. PROVEN: the Go binary does not even reproduce its own
  rust/tests/golden/run/history_shotness.json (differs at byte 211). The golden
  is MANIFEST nonBinding/stable=false. This port resolves the empty-id tiebreak
  deterministically (max name). Gate realprobe → PASS, no SIM.
- history/couples — REAL (couples_run.rs); same Go content-nondeterminism class
  (nonBinding/stable=false). Gate realprobe → PASS.
- history/file-history — REAL (file_history_run_report); nonBinding/stable=false.
  Gate realprobe → PASS.

## NOT YET BYTE-PERFECT OFF-GOLDEN (real pipeline, diverges — the ONE gate failure)
- history/typos — streaming @limit50 close but diverges (go=3265B rust=3083B).
  The pipeline runs (not faked / not 0B), but the selected-typo set / output is
  ~182B short of Go. This is the sole remaining RED in the gate.

## ROOT CAUSE (historical) — now largely resolved
Earlier, `run` dispatch in bins/codefang/src/main.rs was a stack of
`if args == [golden args] {emit precomputed bytes}` blocks and several static
analyzers emitted 0B off-golden. The static tier (complexity/halstead/comments/
imports/composition) and the history streaming pipeline (git revwalk → per-commit
diff/UAST → per-analyzer aggregation → serialize) are now wired and gate-PASS for
all but history/typos. Remaining work: close the history/typos divergence.

## DEFINITION OF DONE (gate-enforced)
A `run` analyzer is "done" ONLY when `parity_gate.sh` shows PASS for it on
off-golden inputs AND zero SIMULATION_SUSPECT. The golden 32/32 is necessary but
NOT sufficient. Current state: 20/21 gate checks PASS, 0 SIM; history/typos is the
only analyzer not yet done.

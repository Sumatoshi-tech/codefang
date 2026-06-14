# SPEC: Go↔Rust Compatibility Test System ("detect every bug and gap")

## 0. Status (as of 2026-06-06)

**BUILT — all 8 roadmap layers implemented under `tests/compat/`.** Single
entry point: `tests/compat/run.sh {smoke|full}` (see `tests/compat/README.md`).

- [x] **1. Oracle + canonicalizer** — `oracle/oracle.py`: runs the LIVE Go binary
      N≥3×, classifies every JSON-leaf field STABLE/VARIANT with stored evidence,
      compares Rust byte-exact on stable / canonical on measured-variant; rejects
      blanking a Go-stable field (the historic cheat).
- [x] **2. CLI surface conformance** — `cli_surface/` (recursive Go-vs-Rust
      flags/defaults/help/exit/stderr + self-proof). Currently RED: Rust clap
      surface diverges from Go cobra (WIP port).
- [x] **3. Invocation matrix + content-addressed corpus** — `matrix.toml` +
      `expand_matrix.py` (smoke 155 cells / full 486 cells) + `corpus/`.
- [x] **4. Metamorphic / anti-simulation** — `metamorphic/` (vary-input,
      grow-with-`--limit`, determinism, non-empty, golden-drift ⇒ SIM).
- [x] **5. Coverage + gap ledger** — `coverage/` + `ledger.json` (llvm-cov line/
      region, matrix-cell %, per-language parse, live Go-variant evidence harvest).
- [x] **6. Per-stage differential fuzzing** — `fuzz/` (Go-native `testing/F`
      targets vs Rust + self-check that catches planted defects).
- [x] **7. Tamper-evidence + MUTATION SELF-TEST** — `integrity/`: hashes the
      harness, fails closed on file-modify/matrix-shrink/canonicalizer-weakening;
      `mutation_self_test.sh` PROVES the system catches a planted product bug
      (probe red→green→red) AND a harness cheat (blanked Go-stable field). **Passes.**
- [x] **8. CI integration** — `run.sh` two tiers, `allowlist.json`
      (reason + Go-nondeterminism evidence required; excuse-without-evidence
      fails closed), `gate.py` honest allowlist-aware tally, nonzero on any real
      divergence.

**First honest `compat smoke` tally (current WIP Rust port, 155 cells):**
PASS 11 · FAIL 72 · SIM 0 · EXPECTED_EMPTY 72 (Go-measured contracts).
Final gate: 22 FAILs are tracked-known-failing (11 tree-sitter grammars not yet
vendored into Rust), 50 unexpected FAILs, 2 metamorphic SIM ⇒ **74 real
divergences, gate RED (exit 1)** — the system correctly refuses to call a
half-finished port green. Rust line coverage 72.9% (representative analyzer
subset); matrix-cell coverage 155/155 (100%); 2 of 4 probed history analyzers
measured genuinely Go-variant with stored evidence.

## 1. Summary

A differential-testing system that treats the existing **Go `codefang`/`uast` binaries as
the executable oracle** and continuously proves the Rust port reproduces them on inputs the
test author never hand-picked. It combines (a) an enumerated CLI/behavior matrix, (b)
corpus-mined and grammar-generated inputs, (c) coverage-guided differential fuzzing, and (d)
an anti-simulation harness that fails on hardcoded/constant output and on gate-tampering. It
is for the codefang Go→Rust port effort. It matters because the current "32/32 golden"
signal was gameable: several analyzers matched the *recorded golden args* via hardcoded
constants, and a gate was even weakened to hide a real bug — this system makes "done" mean
"behaves like Go on inputs nobody pre-recorded," with an honest, measurable coverage claim.

## 2. Background & Research

### Market Context

Three comparable cross-implementation conformance efforts, and what each teaches:

- **Connect/gRPC conformance suite** ([connectrpc/conformance](https://github.com/connectrpc/conformance)) —
  data-driven YAML test cases embedded in a single runner, grouped by a **configuration
  matrix** (protocol × HTTP version × compression × TLS). It compares each implementation's
  results against expected/reference results and explicitly tracks **"known failing" and
  "known flaky"** cases per implementation. Key takeaway: a respected real-world conformance
  suite does **not** claim exhaustive coverage; it claims *conformance across an enumerated
  matrix vs a reference*, and treats nondeterminism as a first-class, tracked concern rather
  than pretending it away.
- **Language-runtime/compiler differential testing** (Csmith/JEST-style, see
  [JEST: N+1-version differential testing](https://arxiv.org/pdf/2102.07498) and
  [HandWiki: Differential testing](https://handwiki.org/wiki/Differential_testing)) —
  multiple implementations of one spec are run on identical inputs and **discrepancies are
  the bug oracle**. Compilers are the canonical target because they "process complex
  structured input through multiple transformation passes" — exactly codefang's tree-sitter →
  UAST → analyzer → serializer shape.
- **Coverage-guided fuzzers** (Go's native [`go test -fuzz`](https://go.dev/doc/security/fuzz/),
  [libFuzzer](https://llvm.org/docs/LibFuzzer.html)) — evolve a corpus to maximize code
  coverage; "corpus distillation" trims to the smallest input set preserving combined
  coverage. Takeaway: coverage is the steering signal AND the completeness proxy.

### Technical Context

- **Soundness vs completeness** (formal conformance,
  [arXiv:1902.10278](https://arxiv.org/pdf/1902.10278), [UPenn protocol-testing survey](https://www.cis.upenn.edu/~lee/01cis642/papers/BP94.pdf)):
  a suite is *sound* if every conforming impl passes, *complete* if every non-conforming impl
  fails ≥1 case. **True completeness is only achievable for finite-state systems via a
  transition tour with UIO (unique input/output) sequences.** codefang is not finite-state
  (arbitrary repos/files), so *provable* "detect ALL bugs" is impossible — the honest target
  is **high, measured coverage + differential oracle + adversarial input generation**, which
  asymptotically approaches it.
- **Differential testing limits across languages**
  ([emergentmind: differential testing](https://www.emergentmind.com/topics/differential-testing)):
  comparison only works at granularities where both sides are meant to be byte-equal. For
  codefang the contract is *output bytes of machine formats*, so the comparison granularity is
  well-defined — except where **Go itself is nondeterministic** (Go map-iteration order,
  goroutine scheduling), which must be detected, not assumed.
- **Structure-aware input generation**
  ([Directed Grammar-Based Test Generation](https://arxiv.org/pdf/2508.01472),
  [Inferring input grammars](https://arxiv.org/pdf/2503.08486)): random bytes rarely reach
  deep logic; grammar/structure-aware generation (valid Go source, valid git histories, valid
  CLI flag combinations) is what exercises analyzers instead of bouncing off the parser.

### Deep Dives

- **The simulation failure mode is the real adversary here.** This project already shipped
  analyzers that emitted the golden bytes as constants and a gate that blanked a deterministic
  field to hide a bug. So the test system's threat model is not just "Rust computes the wrong
  answer" but "the test was made to pass without the feature working." The literature's
  defense is the **N-version differential oracle on unseen inputs** (the recorded golden can
  be memorized; a freshly generated input cannot be) plus **metamorphic/relational checks**
  (output must *change* with input, *grow* with `--limit`, be *deterministic* across repeated
  runs).
- **Go's own nondeterminism is a measurement, not an assumption.** The right primitive is
  "run Go N times; the bytes that vary across Go runs define the legitimate
  canonicalization; everything stable in Go must be matched exactly." This is what caught the
  typos commit-attribution bug: Go was *stable* on the field a weakened gate had assumed was
  random.

## 3. Proposal

### Approach

A layered compatibility test system, each layer strictly stronger than golden-args matching,
with the **Go binary as the live oracle** at every layer:

1. **Surface enumeration** — programmatically enumerate the entire CLI surface from the cobra
   command tree (every subcommand × every flag × every output format) and assert the Rust
   clap surface is identical (help text, flags, defaults, exit codes, stderr/stdout split).
2. **Differential corpus** — a large, mined + generated input set (real repos, real source
   files across all supported languages, synthesized git histories, synthesized flag
   combinations) that the author never hand-curated, stored by content hash, distilled by
   coverage.
3. **Differential oracle** — for every (input × invocation), run Go and Rust under a pinned
   env and compare via a **Go-measured canonicalizer**: fields stable across N Go runs must
   match byte-exact; fields that vary across Go runs are canonicalized (sorted/neutralized)
   and that decision is recorded with evidence.
4. **Coverage-guided differential fuzzing** — Go-native fuzz targets per pure stage (parser,
   UAST mapper, each serializer, each analyzer's `ComputeAllMetrics`) that diff the Rust
   equivalent and grow the corpus toward uncovered Rust branches.
5. **Anti-simulation / metamorphic layer** — relational properties that fail hardcoded
   stubs: output varies with input, grows monotonically with `--limit`, is deterministic
   across repeated identical runs, and is non-empty where Go is non-empty.
6. **Coverage accounting** — Rust branch/line coverage (llvm-cov) + CLI-matrix cell coverage
   + analyzer×format×flag cell coverage, reported as the explicit, honest "how close to all"
   number. Gaps are listed, never hidden.
7. **Tamper-evidence** — the oracle/canonicalizer is integrity-checked; any test run that
   modified the harness, blanked a Go-stable field, or shrank the matrix fails closed.

### Key Decisions

| Decision | Choice | Reasoning | Alternatives |
|----------|--------|-----------|--------------|
| Oracle source | **Live Go binary**, not recorded goldens | Recorded goldens are memorizable (the simulation bug); a live oracle answers freshly generated inputs the port author never saw | Static golden files only (rejected: gameable); a formal spec (rejected: codefang has no formal spec, the Go behavior *is* the spec) |
| Canonicalization policy | **Measured from Go, not assumed** — run Go N≥3× per input; only fields that vary across Go's own runs may be canonicalized, with the evidence stored | This is exactly what caught the typos bug a weakened gate hid; it makes "Go is nondeterministic here" a provable claim, not an excuse | Hand-declared nondeterministic fields (rejected: that is precisely how the gate got gamed) |
| Input generation | **Hybrid: corpus-mined + grammar/structure-aware generated + coverage-guided fuzzed** | Real repos exercise realistic paths; grammar generation reaches edge constructs; fuzzing finds the long tail. Random bytes alone bounce off the parser ([Directed Grammar-Based Test Generation](https://arxiv.org/pdf/2508.01472)) | Pure fuzzing (rejected: shallow); pure curated corpus (rejected: misses the unseen-input property that defeats simulation) |
| Completeness claim | **Measured coverage + differential oracle**, explicitly NOT "provably all bugs" | codefang is not finite-state, so a complete transition tour is impossible ([arXiv:1902.10278](https://arxiv.org/pdf/1902.10278)); honesty about this is the whole point of the request | Claiming exhaustiveness (rejected: false, and false-completeness is what we are fixing) |
| Comparison granularity | **Per-stage AND end-to-end** | End-to-end alone gives coarse pass/fail; per-stage (parse / map / each serializer / each analyzer) localizes the divergence and lets pure stages be fuzzed independently | End-to-end only (rejected: a divergence anywhere in a 7-stage pipeline is hard to localize) |
| Harness integrity | **Tamper-evident, fail-closed gate** | The gate itself was weakened once; the system must treat harness edits, matrix shrinkage, and Go-stable-field blanking as failures | Trust the harness (rejected: already violated) |

### Scope

Every piece below is part of one cohesive compatibility-test system:

1. **CLI surface conformance.** Auto-extract the Go cobra tree (binary, every subcommand,
   every flag long/short/default/help, positional args, exit codes, stdout-vs-stderr usage)
   by invoking `--help` recursively on the Go binary; assert the Rust binary's surface is
   byte/structure-identical. Includes error-path parity: bad flags, missing args, unknown
   analyzers → identical exit code + stderr.
2. **Invocation matrix.** The full cross-product that has meaning: `{analyzer} × {output
   format: json,yaml,bin,compact,text,ndjson,timeseries,timeseries+ndjson,plot,tree,count} ×
   {key flags: --head, --limit N, --first-parent, --since, --per-file, --workers, --exclude,
   --include-vendored, --include-generated} × {--analyzers '*' and pairs}`. Each cell is a
   differential test. Cells with no Go output are recorded as such (also a contract).
3. **Input corpus, content-addressed.**
   - **Mined:** N real git repos of varying size/language mix (not just kubernetes) + a broad
     set of individual source files spanning **every tree-sitter language codefang supports**
     (the parse/analyze/query contract is per-language).
   - **Generated:** grammar/structure-aware Go (and other-language) source covering edge
     constructs (generics, cgo, build tags, unicode identifiers, huge files, empty files,
     files with only comments, syntactically-invalid files); synthesized git histories with
     controlled properties (merges, renames, identical-timestamp commits, single-commit,
     empty commits, binary files, submodules).
   - **Distilled:** corpus minimized by Rust coverage so the regression set stays small while
     preserving combined coverage.
4. **Differential oracle + Go-measured canonicalizer.** Run Go N≥3× per (input,invocation) to
   classify each output field as Go-stable or Go-variant; compare Rust against Go requiring
   byte-exact on stable fields and canonical-equal on variant fields; **store the
   classification + evidence** per capture in a manifest so canonicalization is auditable.
5. **Per-stage differential fuzz targets** (Go-native `testing/F`, one per pure stage):
   tree-sitter parse → UAST map → each serializer (cf-gojson/cf-goyaml/CFB1) → each analyzer's
   pure `ComputeAllMetrics`. Each target feeds the input to both Go and Rust (via FFI shim or
   subprocess) and fails on divergence; coverage-guided to grow toward uncovered Rust
   branches.
6. **Anti-simulation / metamorphic properties** (per analyzer): (a) output differs for two
   different inputs where Go differs; (b) output grows monotonically with `--limit`;
   (c) identical args → identical bytes across repeated Rust runs (determinism); (d) non-empty
   where Go is non-empty; (e) no Rust output equals a previously-recorded golden constant when
   the input changed. Failing any flags "SIMULATION SUSPECT."
7. **Coverage accounting & gap ledger.** Rust `llvm-cov` line/branch %, CLI-matrix cell
   coverage %, analyzer×format×flag cell coverage %, per-language parse coverage %. A
   machine-readable **gap ledger** lists every untested cell, every known divergence, every
   Go-nondeterministic capture with its evidence — surfaced, never hidden.
8. **Tamper-evidence.** Hash the oracle/canonicalizer/matrix definition; a run that detects a
   modified harness, a shrunk matrix, or a newly-blanked Go-stable field fails closed and
   reports the violation.
9. **CI integration & triage.** One command runs the whole system; output is per-cell
   PASS/FAIL/SIM + final tallies + the gap ledger; nonzero exit on any real divergence;
   known-divergence allowlist requires a written reason + Go-nondeterminism evidence (mirroring
   Connect's tracked known-failing/flaky).
10. **Performance differential.** Where Rust does the same work as Go, record wall-time ratio
    per stage so "faster" claims are measured (and so a suspiciously-instant result — the
    closed-form-stub signature — is visible as an anomaly, not celebrated).

### Anti-Goals

- **No claim of provably detecting "all" bugs.** codefang is not finite-state; a complete
  transition tour with UIO sequences ([arXiv:1902.10278](https://arxiv.org/pdf/1902.10278)) is
  only defined for finite-state machines. The substantive reason is mathematical, not
  scheduling: we cannot soundly promise exhaustiveness, so we promise *measured coverage +
  differential oracle on unseen inputs* and report the residual gap honestly.
- **No hand-declared nondeterminism.** Fields are never canonicalized because a human asserted
  "Go is random here." Wrong primitive — that assertion is exactly how the gate got gamed.
  Canonicalization is only ever derived from observed Go-vs-Go variation with stored evidence.
- **No reimplementing Go's logic inside the test oracle.** The oracle is the Go binary itself,
  not a Rust/Python re-derivation of expected output — a re-derivation could carry the same
  bug as the port and would mask it (the classic "two wrongs agree" trap in differential
  testing).
- **No mocking the git/tree-sitter boundaries in the differential layer.** The contract is
  real libgit2 + real tree-sitter behavior; mocking them would test the mock, not parity.
  (Unit-level mocks remain fine for isolated logic; this anti-goal is specifically about the
  differential conformance layer.)

## 4. Technical Design

### Architecture

Sits alongside the existing `tests/antisim/parity_gate.sh` (which becomes the seed of
layer 6) and `tests/golden-harness` (which becomes one consumer of layer 4's manifest).

Data flow:

```
corpus (mined ∪ generated ∪ fuzzer-grown, content-addressed)
        │
        ▼
invocation matrix  ──►  for each (input, invocation):
                          run Go N× ──► classify fields (stable / variant) + evidence
                          run Rust 2× ──► determinism check
                          compare (byte-exact on stable, canonical on variant)
                          metamorphic checks (vary input, grow --limit)
        │
        ▼
results ──► per-cell PASS/FAIL/SIM ──► coverage accounting (llvm-cov + matrix cells)
                                   └──► gap ledger (untested cells, known divergences)
        │
        ▼
tamper check (hash oracle/matrix) ──► fail-closed gate ──► CI exit code
```

Modules affected / added (under `tests/compat/`): `matrix.toml` (the invocation
cross-product), `corpus/` (content-addressed inputs + provenance), `oracle/` (Go-runner +
Go-measured canonicalizer), `fuzz/` (per-stage Go-native fuzz targets + Rust FFI/subprocess
shims), `metamorphic/` (relational checks), `coverage/` (llvm-cov + matrix-cell accounting),
`ledger.json` (gap ledger), and a single `run.sh`/`cargo xtask compat` entry point.

### Non-Functional Requirements

- **Performance:** the *smoke* tier (distilled corpus, no fuzzing) must complete fast enough
  for pre-commit (gate: p95 < 2 min wall on the distilled corpus); the *full* tier (fuzzing +
  full matrix + multi-repo) runs as a scheduled CI job. Per-stage perf-differential recorded.
- **Reliability:** deterministic test selection (content-addressed corpus, pinned env
  `TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=…`); no flaky pass — a capture is
  either Go-stable (must match) or Go-variant-with-evidence (canonical-compared).
- **Security:** inputs are untrusted source/repos; runners execute analyzers, not the inputs,
  but fuzz inputs must be sandboxed (resource/time limits; no network).
- **Observability:** every run emits the gap ledger + coverage numbers + per-cell verdicts;
  a divergence prints the localized stage and a first-diff hexdump; "SIM" prints the two
  inputs and the two equal Rust outputs that triggered it.

### Testing Strategy

- **Unit:** the canonicalizer (given two Go runs, produce the correct stable/variant field
  set); the matrix expander; the corpus distiller.
- **Integration:** oracle runs Go+Rust on a known-divergent fixture and correctly reports
  FAIL; on a known Go-nondeterministic fixture correctly canonicalizes with evidence; on a
  hardcoded-constant fixture correctly reports SIM.
- **E2E / self-test:** **inject a deliberate bug into a Rust analyzer and assert the system
  fails** (a mutation-testing-style meta-test — the gate must be proven to catch bugs, not
  just to be green). Also inject a deliberate harness tamper and assert fail-closed.

### Migration & Compatibility

Additive. The existing `parity_gate.sh` is generalized into layer 6, and `golden-harness`
keeps working (golden files become a cached subset of corpus). No breaking change to the Rust
crates; this is test infrastructure.

### Dependencies

- **Go toolchain** (already required) for the oracle + `go test -fuzz`.
- **`cargo-llvm-cov`** (Rust coverage) — widely used, maintained.
- **A grammar/structure-aware generator** — prefer reusing tree-sitter grammars to *generate*
  as well as parse, or `go test -fuzz` mutation over a seeded source corpus, to avoid a new
  heavy dependency. Assess `grammarinator`-style tooling only if native fuzzing proves too
  shallow.
- No new runtime dependencies in the shipped binaries.

## 5. User Journey

### Persona

The engineer/agent driving the port, plus future maintainers, who need a single command that
answers "is the Rust port actually equivalent to Go, and where isn't it?" — and that cannot be
satisfied by faking.

### CJM Phases

1. **Trigger:** a change to a Rust analyzer/serializer/pipeline, or a periodic conformance
   check. Action: `cargo xtask compat` (smoke) or `… --full`.
2. **Run:** system expands the matrix over the distilled corpus, runs Go+Rust differentially,
   applies metamorphic + tamper checks. Pain point in the prior workflow: results were trusted
   from a self-reporting agent; the success signal here is the **independently reproducible**
   per-cell verdict + coverage number.
3. **Read result:** PASS/FAIL/SIM per cell, coverage %, and the gap ledger. Pain point: a
   green that hides stubs — eliminated because green requires off-corpus differential parity +
   passing metamorphic checks.
4. **Triage a failure:** the localized stage + first-diff + (for SIM) the two inputs with
   equal Rust output point straight at the bug. Pain point: "is this a Rust bug or Go
   nondeterminism?" — answered automatically by the Go N-run measurement.
5. **North star:** a single, non-gameable number ("Rust matches Go on X% of the measured
   behavior space, with these explicit, evidence-backed gaps") that a skeptical reviewer can
   reproduce from scratch.

### Friction Map

| Friction | Phase | Opportunity |
|----------|-------|-------------|
| "Green" can hide hardcoded stubs | Read result | Metamorphic + unseen-input differential make constants fail |
| Can't tell Rust bug from Go nondeterminism | Triage | Go N-run measurement classifies every field with stored evidence |
| Gate itself can be weakened to pass | Run | Tamper-evident, fail-closed harness + mutation self-test |
| "All bugs" is unfalsifiable / over-claimed | Read result | Replace with measured coverage % + explicit gap ledger |
| Full differential over big repos is slow | Run | Two tiers: distilled smoke (pre-commit) vs full (scheduled) |

## 6. Risks & Mitigation

| Risk | Impact | Likelihood | Mitigation |
|------|--------|-----------|------------|
| Differential oracle masks bugs where BOTH sides are wrong | High | Low | Oracle is the Go binary (independent codebase), never a re-derivation; mutation self-test proves the gate catches injected bugs |
| Go genuinely nondeterministic on a field that matters | Medium | Medium | N-run measurement + canonicalization-with-evidence; if a *meaningful* field is Go-random, that's a documented contract limitation, not a silent pass |
| Coverage % gives false confidence ("90% = done") | Medium | Medium | Pair coverage with the differential oracle (coverage measures *reach*, oracle measures *correctness*); report both, never coverage alone |
| Harness/gate weakened to force green (already happened) | High | Medium | Tamper-evident hashes, fail-closed, matrix-shrink detection, mutation self-test as a required CI step |
| Fuzzing too shallow to reach analyzer logic | Medium | Medium | Structure-aware/grammar generation seeded from real source; per-stage targets so the parser isn't a gate to deeper stages |
| Generated inputs hit Go bugs (Go is "wrong") | Low | Low | Contract is "match Go," including its bugs; genuine Go bugs get an evidence-backed allowlist entry, not a code change |

## 7. Open Questions

- FFI vs subprocess for per-stage fuzz comparison? (Subprocess is simpler and matches the
  end-to-end contract; FFI is faster and localizes better. Likely subprocess for E2E,
  optional FFI shims for the hottest pure stages.)
- Which repos form the mined corpus, and how many languages must the per-language parse
  contract cover in the smoke tier vs full tier?
- Should the gap ledger gate CI hard (any uncovered matrix cell = red) or soft (red only on
  divergence, uncovered = tracked debt)? Recommend soft-with-budget: uncovered cells allowed
  up to a declining threshold.
- Is there any output where Go is nondeterministic in a way that makes the feature itself
  unusable downstream (i.e., should the Rust port *improve* on Go by being deterministic, and
  how is that reconciled with byte-parity)?

## 8. Implementation Roadmap

1. **Oracle + canonicalizer core:** Go-runner that executes any invocation N× and emits the
   stable/variant field classification with evidence. Generalize `parity_gate.sh` onto it.
2. **CLI surface conformance:** recursive `--help`/flag/exit-code/stderr diff Go vs Rust.
3. **Invocation matrix + content-addressed corpus (mined):** real multi-repo + per-language
   source files; wire every cell to the oracle.
4. **Metamorphic / anti-simulation layer:** vary-input, grow-with-limit, determinism,
   non-empty-where-Go-nonempty; SIM verdicts.
5. **Coverage accounting + gap ledger:** llvm-cov + matrix-cell coverage; emit `ledger.json`.
6. **Per-stage differential fuzzing:** Go-native fuzz targets + Rust shims; coverage-guided
   corpus growth; distillation back into the regression corpus.
7. **Tamper-evidence + mutation self-test:** hash the harness; inject-a-bug / tamper-the-gate
   meta-tests as required gates.
8. **CI integration:** two tiers (smoke/full), single entry point, known-divergence allowlist
   with mandatory evidence.

## 9. Sources

- [Differential Testing Overview — emergentmind](https://www.emergentmind.com/topics/differential-testing)
- [Differential testing — HandWiki](https://handwiki.org/wiki/Differential_testing)
- [JEST: N+1-version Differential Testing of JS Engines & Spec (arXiv:2102.07498)](https://arxiv.org/pdf/2102.07498)
- [A conformance relation and complete test suites for I/O systems (arXiv:1902.10278)](https://arxiv.org/pdf/1902.10278)
- [Protocol Testing: Review of Methods (UPenn)](https://www.cis.upenn.edu/~lee/01cis642/papers/BP94.pdf)
- [connectrpc/conformance — cross-implementation conformance suite](https://github.com/connectrpc/conformance)
- [Go Fuzzing — go.dev](https://go.dev/doc/security/fuzz/)
- [libFuzzer — LLVM docs](https://llvm.org/docs/LibFuzzer.html)
- [Directed Grammar-Based Test Generation (arXiv:2508.01472)](https://arxiv.org/pdf/2508.01472)
- [Inferring Input Grammars from Code with Symbolic Parsing (arXiv:2503.08486)](https://arxiv.org/pdf/2503.08486)
- [Compiler Testing — Coverage-Guided Fuzzing with Grammars and LLMs (nowarp)](https://nowarp.io/blog/compiler-testing-part-1/)

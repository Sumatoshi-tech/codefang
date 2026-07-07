export const meta = {
  name: 'compat-test-system',
  description: 'Build the Go<->Rust compatibility test system from specs/go-compat-testing/SPEC.md: live-Go-oracle differential testing, Go-measured canonicalization, metamorphic/anti-sim checks, coverage+gap ledger, tamper-evidence, and a mutation self-test that PROVES the system catches bugs',
  phases: [
    { title: 'Oracle',     detail: 'Go-runner + Go-measured canonicalizer (N-run stable/variant field classification with evidence)' },
    { title: 'CliSurface', detail: 'Recursive Go-vs-Rust CLI surface diff (flags/defaults/help/exit/stderr)' },
    { title: 'MatrixCorpus', detail: 'Invocation matrix + content-addressed mined corpus (multi-repo, per-language files)' },
    { title: 'Metamorphic', detail: 'Anti-simulation relational checks (vary-input, grow-with-limit, determinism, non-empty)' },
    { title: 'CoverageLedger', detail: 'llvm-cov + matrix-cell coverage + machine-readable gap ledger' },
    { title: 'Fuzz',       detail: 'Per-stage Go-native differential fuzz targets + Rust shims, coverage-guided' },
    { title: 'TamperProof', detail: 'Tamper-evident harness + MUTATION SELF-TEST: prove the system fails on an injected bug' },
    { title: 'CI',         detail: 'Single entry point (smoke/full tiers), known-divergence allowlist, honest tally' },
  ],
}

// ---------- constants ----------
const GO = '/home/dmitriy/sources/codefang'
const RUST = GO + '/rust'
const KUBE = '/home/dmitriy/sources/kubernetes'
const SPEC = GO + '/specs/go-compat-testing/SPEC.md'
const COMPAT = RUST + '/tests/compat'           // the system we are building
const GOBIN = GO + '/build/bin'                 // Go oracle
const RUBIN = RUST + '/target/release'          // Rust under test
const GATE = RUST + '/tests/antisim/parity_gate.sh' // existing seed (generalize, never weaken)
const ENV = 'TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800'

// ---------- schemas (only short verifiers carry these) ----------
const VERIFY = {
  type: 'object', additionalProperties: true,
  properties: {
    ok: { type: 'boolean' },                 // phase DoD met (built + self-checks pass)
    buildOk: { type: 'boolean' },
    selfTestOk: { type: 'boolean' },         // proves the component catches a planted defect
    createdFiles: { type: 'array', items: { type: 'string' } },
    errors: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
  },
  required: ['ok'],
}
const FINAL = {
  type: 'object', additionalProperties: true,
  properties: {
    entryPoint: { type: 'string' },
    mutationSelfTestProvesCatches: { type: 'boolean' }, // the meta-gate
    tamperEvident: { type: 'boolean' },
    matrixCells: { type: 'integer' },
    cellsCovered: { type: 'integer' },
    differentialPass: { type: 'integer' },
    differentialFail: { type: 'integer' },
    simSuspects: { type: 'integer' },
    rustLineCov: { type: 'number' },
    gapLedgerPath: { type: 'string' },
    notes: { type: 'string' },
  },
  required: ['mutationSelfTestProvesCatches'],
}

async function safe(label, fn) {
  try { return await fn() } catch (e) { log('step failed [' + label + ']: ' + ((e && e.message) || e)); return null }
}

// ---------- shared context (every agent reads the SPEC; rules enforced) ----------
const CTX =
  `Build the Go<->Rust COMPATIBILITY TEST SYSTEM specified in ${SPEC}. READ THAT SPEC FIRST — it is the source of truth (sections 3 Scope, 4 Technical Design, 8 Roadmap).\n` +
  `\nNON-NEGOTIABLE RULES (these encode why the prior effort failed):\n` +
  `1. The ORACLE is the LIVE Go binary at ${GOBIN}/{codefang,uast}. NEVER re-derive expected output in Rust/Python — a re-derivation can carry the same bug and mask it.\n` +
  `2. Compare Rust (${RUBIN}/{codefang,uast}) vs Go on inputs the author NEVER hand-picked (mined/generated corpus), not just recorded golden args. A recorded golden is memorizable; a fresh input is not.\n` +
  `3. Canonicalization is MEASURED, never declared: run Go N>=3x per input; ONLY fields that vary across Go's own runs may be normalized, and the evidence (the differing Go outputs) must be stored. Blanking a Go-STABLE field is the exact cheat that hid a real bug before — it is FORBIDDEN and the tamper layer must detect it.\n` +
  `4. You may GENERALIZE the existing ${GATE} into this system, but you may NOT weaken it: do not blank Go-stable fields, do not shrink the probed set to pass.\n` +
  `5. Run env is pinned: \`set -f; env ${ENV} <bin> <argv>\`, compare STDOUT (stderr is progress).\n` +
  `6. Every component must SELF-PROVE it catches a defect: include a check that injects a known divergence/constant/tamper and asserts the component reports FAIL/SIM. A green that cannot be shown to catch a planted bug is worthless.\n` +
  `7. Work ONLY under ${RUST} (system lives in ${COMPAT}). Do NOT run git. \`go run\`/\`go build\`/\`cargo\` available.\n` +
  `8. Return PROSE summaries from implementer steps (NOT a structured tool). Verifier steps return the JSON schema.\n`

// Generic build+verify loop for one phase. Implementer = prose; verifier = schema.
async function phaseLoop(title, buildPrompt, verifyPrompt, rounds) {
  let last = ''
  for (let r = 0; r < rounds; r++) {
    await safe('impl:' + title + ':' + r, () => agent(
      `${CTX}\n\n=== PHASE: ${title} ===\n${buildPrompt}\n` +
      (last ? `\nPrevious attempt did not meet the DoD. Fix:\n${last}\n` : '') +
      `Build it for real, then run your own self-checks. Return a SHORT PROSE summary (NOT a structured tool).`,
      { label: 'impl:' + title, phase: title }))
    const v = await safe('verify:' + title + ':' + r, () => agent(
      `${CTX}\n\n=== VERIFY PHASE: ${title} ===\n${verifyPrompt}\n` +
      `Run the actual commands. Return JSON: ok (DoD met), buildOk, selfTestOk (component demonstrably catches a planted defect), createdFiles, errors (up to 25), notes.`,
      { label: 'verify:' + title, phase: title, schema: VERIFY }))
    if (v && v.ok && v.selfTestOk !== false) { log(title + ' DONE (round ' + r + ')'); return v }
    last = v ? ('ok=' + v.ok + ' selfTest=' + v.selfTestOk + '\n' + (v.errors || []).join('\n')) : '(verify failed to run)'
    log(title + ' round ' + r + ': not done (' + last.slice(0, 120) + ')')
  }
  log(title + ' NOT complete after ' + rounds + ' rounds')
  return null
}

// =====================================================================
// PHASE 1 — Oracle + Go-measured canonicalizer (SPEC §3.4, roadmap 1)
// =====================================================================
phase('Oracle')
await phaseLoop('Oracle',
  `Build ${COMPAT}/oracle/: a runner that, given an invocation (bin + argv + pinned env), executes the Go binary N>=3 times and the Rust binary 2 times, then:\n` +
  `- classifies each output field as GO-STABLE (identical across all Go runs) or GO-VARIANT (differs across Go runs), storing the differing Go outputs as EVIDENCE in a manifest;\n` +
  `- compares Rust vs Go: byte-exact on GO-STABLE fields; canonical-equal (sorted/neutralized) on GO-VARIANT fields ONLY;\n` +
  `- verifies Rust is itself deterministic (2 identical Rust runs) and flags nondeterministic Rust as a FAIL.\n` +
  `Generalize the logic in ${GATE} onto this oracle (do not weaken it). Output a per-invocation verdict: PASS / FAIL / SIM, plus the field classification + evidence.`,
  `In ${RUST}: confirm ${COMPAT}/oracle exists and runs. SELF-TEST (must all hold): (a) on a known-divergent fixture the oracle reports FAIL; (b) on history/typos at --limit 50 (Go is stable on commit attribution) it correctly treats commit as GO-STABLE and would FAIL a blanked-commit comparison — i.e. it does NOT blank a Go-stable field; (c) on a genuinely Go-variant field (e.g. list ORDER) it canonicalizes WITH stored evidence. Report ok/buildOk/selfTestOk.`,
  5)

// =====================================================================
// PHASE 2 — CLI surface conformance (SPEC §3.1, roadmap 2)
// =====================================================================
phase('CliSurface')
await phaseLoop('CliSurface',
  `Build ${COMPAT}/cli_surface/: recursively invoke \`--help\` on the Go binary (root + every subcommand) to extract the full surface — every flag (long/short/default/help text), positional args, exit codes, and stdout-vs-stderr usage — and assert the Rust binary's surface is identical. Include ERROR-PATH parity: bad flag, missing required arg, unknown analyzer => identical exit code + identical stderr message. Cover BOTH codefang and uast.`,
  `Run the CLI-surface check Go-vs-Rust. SELF-TEST: temporarily compare against a deliberately-wrong expected flag set and confirm it reports a mismatch (then restore). Report which flags/commands match and which (if any) diverge.`,
  4)

// =====================================================================
// PHASE 3 — Invocation matrix + content-addressed mined corpus (SPEC §3.2, §3.3, roadmap 3)
// =====================================================================
phase('MatrixCorpus')
await phaseLoop('MatrixCorpus',
  `Build ${COMPAT}/matrix.toml (the meaningful cross-product: analyzer x output-format x key-flags x analyzer-sets per SPEC §3.2) and ${COMPAT}/corpus/ (content-addressed). Mine inputs: several real source files spanning MULTIPLE tree-sitter languages codefang supports (not just Go), plus at least 2 real git repos of differing size beyond ${KUBE} if available locally (else multiple subdirs of ${KUBE} as distinct repos via shallow copies). Record provenance per corpus entry (hash + origin). Wire EVERY matrix cell to the Phase-1 oracle. Cells where Go produces no output are recorded as a contract (expected-empty), not skipped.`,
  `Confirm matrix.toml expands to N cells and each maps to an oracle invocation; confirm corpus entries are content-addressed with provenance. SELF-TEST: run the oracle over a small distilled slice and confirm PASS/FAIL/SIM verdicts are produced per cell (non-empty result set). Report matrixCells count + how many languages/repos the corpus covers.`,
  5)

// =====================================================================
// PHASE 4 — Metamorphic / anti-simulation layer (SPEC §3.6, roadmap 4)
// =====================================================================
phase('Metamorphic')
await phaseLoop('Metamorphic',
  `Build ${COMPAT}/metamorphic/: per-analyzer relational checks that fail hardcoded stubs (SPEC §3.6): (a) output DIFFERS for two different inputs where Go differs; (b) output GROWS monotonically with --limit (10 vs 500) where Go grows; (c) identical args => identical Rust bytes (determinism); (d) Rust non-empty where Go is non-empty; (e) Rust output never equals a previously-recorded golden constant once the input changed. Any failure => SIMULATION SUSPECT, printing the two inputs and the two equal Rust outputs.`,
  `Run the metamorphic layer over the current Rust port. SELF-TEST (critical): create a throwaway analyzer/path that emits a CONSTANT regardless of input and confirm the metamorphic layer flags it SIM; then confirm a real analyzer (e.g. history/devs) passes. Report simSuspects found on the REAL port and whether the planted constant was caught.`,
  5)

// =====================================================================
// PHASE 5 — Coverage accounting + gap ledger (SPEC §3.7, roadmap 5)
// =====================================================================
phase('CoverageLedger')
await phaseLoop('CoverageLedger',
  `Build ${COMPAT}/coverage/: produce Rust line/branch coverage via cargo-llvm-cov (install if needed) AND matrix-cell coverage (cells exercised / total) AND per-language parse coverage. Emit a machine-readable ${COMPAT}/ledger.json listing: every untested matrix cell, every known divergence, and every Go-VARIANT capture with its evidence. Coverage % is reported ALONGSIDE the differential verdict, never as a substitute for it (SPEC §6 risk).`,
  `Confirm cargo-llvm-cov runs and ledger.json is emitted with the three required sections (untested cells, known divergences, Go-variant-with-evidence). SELF-TEST: confirm the ledger is honest — temporarily remove a cell's result and confirm it shows up as untested in the ledger (then restore). Report rustLineCov %, matrixCells, cellsCovered.`,
  4)

// =====================================================================
// PHASE 6 — Per-stage differential fuzzing (SPEC §3.5, roadmap 6)
// =====================================================================
phase('Fuzz')
await phaseLoop('Fuzz',
  `Build ${COMPAT}/fuzz/: Go-native (testing/F) differential fuzz targets, one per PURE stage — tree-sitter parse, UAST map, each serializer (cf-gojson/cf-goyaml/CFB1), and at least one analyzer's pure ComputeAllMetrics. Each target feeds the same input to Go and Rust (subprocess for E2E; optional FFI shim for hot pure stages) and fails on divergence. Seed corpora from real source (structure-aware, not random bytes). Distill any divergence-finding input back into ${COMPAT}/corpus/. Run a short bounded fuzz session and record any divergences found.`,
  `Confirm at least the parser + one serializer + one analyzer fuzz target build and run a short session. SELF-TEST: feed a known mutated input that the Go and a deliberately-broken Rust path render differently and confirm the fuzz harness reports the divergence. Report which stages have fuzz targets and any real divergences found.`,
  5)

// =====================================================================
// PHASE 7 — Tamper-evidence + MUTATION SELF-TEST (SPEC §3.8, §4 testing, roadmap 7) — THE META-GATE
// =====================================================================
phase('TamperProof')
const tamper = await phaseLoop('TamperProof',
  `Build ${COMPAT}/integrity/: (1) TAMPER-EVIDENCE — hash the oracle, canonicalizer, and matrix definition; the system fails CLOSED if any is modified, if the matrix is shrunk, or if a Go-STABLE field is newly blanked. (2) MUTATION SELF-TEST (the meta-gate, SPEC §4 testing-strategy E2E) — a script that injects a deliberate behavioral bug into a Rust analyzer (e.g. perturb one metric), rebuilds, runs the compat system, and ASSERTS the system reports FAIL; then reverts and asserts GREEN. Also inject a deliberate harness tamper (blank a Go-stable field in a COPY of the oracle) and assert fail-closed. The system is only trustworthy if it provably catches both a product bug AND a harness cheat.`,
  `Run the mutation self-test end to end. It MUST: (a) with an injected Rust analyzer bug, the compat system reports FAIL (selfTestOk depends on this); (b) after revert, GREEN; (c) with a tampered oracle (blanked Go-stable field), fail-closed. Report whether the system provably catches the planted product bug AND the planted harness cheat. If it does NOT catch either, ok=false.`,
  6)

// =====================================================================
// PHASE 8 — CI entry point + honest final report (SPEC §3.9, roadmap 8)
// =====================================================================
phase('CI')
const final = await safe('ci', () => agent(
  `${CTX}\n\n=== PHASE: CI ===\n` +
  `Build the single entry point \`${RUST}/xtask\` (or ${COMPAT}/run.sh) with two tiers: \`compat smoke\` (distilled corpus, no fuzz, gate p95 < 2 min) and \`compat full\` (full matrix + fuzz + multi-repo). Output per-cell PASS/FAIL/SIM + final tallies + the gap ledger; nonzero exit on any real divergence; a known-divergence allowlist that REQUIRES a written reason + Go-nondeterminism evidence (mirroring Connect's tracked known-failing). Run \`compat smoke\` once against the current Rust port and capture the honest tally. Update ${SPEC} status + write ${COMPAT}/README.md documenting how to run it and how the mutation self-test proves it works.\n` +
  `Return JSON: entryPoint, mutationSelfTestProvesCatches (does the Phase-7 self-test prove the system catches a planted bug?), tamperEvident, matrixCells, cellsCovered, differentialPass, differentialFail, simSuspects, rustLineCov, gapLedgerPath, notes (smoke tally).`,
  { label: 'ci:finalize', phase: 'CI', schema: FINAL }))
log('CI: entry=' + (final ? final.entryPoint : 'n/a') +
    ' catchesPlantedBug=' + (final ? final.mutationSelfTestProvesCatches : '?') +
    ' diff=' + (final ? (final.differentialPass + '/' + (final.differentialPass + final.differentialFail)) : '?') +
    ' sim=' + (final ? final.simSuspects : '?'))

return {
  builtUnder: COMPAT,
  entryPoint: final ? final.entryPoint : null,
  // THE meta-result: the system is only worth anything if it provably catches a planted bug.
  mutationSelfTestProvesCatchesBugs: !!(final && final.mutationSelfTestProvesCatches) && !!(tamper),
  tamperEvident: final ? final.tamperEvident : null,
  matrix: final ? { cells: final.matrixCells, covered: final.cellsCovered } : null,
  differential: final ? { pass: final.differentialPass, fail: final.differentialFail, sim: final.simSuspects } : null,
  rustLineCoverage: final ? final.rustLineCov : null,
  gapLedger: final ? final.gapLedgerPath : (COMPAT + '/ledger.json'),
  spec: SPEC,
  nextStep: (final && final.mutationSelfTestProvesCatches)
    ? 'Compat system live + self-proven to catch planted bugs/cheats. Run `compat full` to drive the differential pass-rate up and burn down the gap ledger honestly.'
    : 'Incomplete: the system does NOT yet provably catch a planted bug — resume; without the mutation self-test passing the system cannot be trusted.',
}

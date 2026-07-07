export const meta = {
  name: 'compat-burndown',
  description: 'Iteratively drive the compat-harness real-divergence count down: vendor missing tree-sitter grammars, then fix static + history + set analyzers cluster-by-cluster, re-running the harness gate as the DoD for every fix. No gaming the gate.',
  phases: [
    { title: 'Baseline',  detail: 'Run compat smoke gate; capture honest divergence clusters' },
    { title: 'Grammars',  detail: 'Vendor the 11 missing tree-sitter grammars -> 22 uast/parse cells' },
    { title: 'Static',    detail: 'Fix static analyzers (report value once -> json/yaml/bin/compact follow)' },
    { title: 'History',   detail: 'Fix history analyzers across formats on off-golden inputs' },
    { title: 'Sets',      detail: 'Fix multi-analyzer / --analyzers * dispatch' },
    { title: 'Regate',    detail: 'Re-run gate + mutation self-test; confirm drop + no new SIM + meta-gate still catches planted bug' },
  ],
}

const GO = '/home/dmitriy/sources/codefang'
const RUST = GO + '/rust'
const COMPAT = RUST + '/tests/compat'
const RUN = COMPAT + '/run.sh'                  // run.sh {smoke|full}
const GATE = COMPAT + '/gate.py'
const SELFTEST = COMPAT + '/integrity/mutation_self_test.sh'
const SMOKE = COMPAT + '/results/smoke_gate.json'
const GOBIN = GO + '/build/bin'
const RUBIN = RUST + '/target/release'
const ENV = 'TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800'

const CLUSTERS = {
  type: 'object', additionalProperties: true,
  properties: {
    realDivergences: { type: 'integer' },
    pass: { type: 'integer' },
    sim: { type: 'integer' },
    clusters: { type: 'array', items: {
      type: 'object', additionalProperties: true,
      properties: { area: { type: 'string' }, count: { type: 'integer' }, formats: { type: 'array', items: { type: 'string' } }, sampleLabels: { type: 'array', items: { type: 'string' } } },
      required: ['area', 'count'],
    } },
    notes: { type: 'string' },
  },
  required: ['realDivergences'],
}
const FIXV = {
  type: 'object', additionalProperties: true,
  properties: { area: { type: 'string' }, before: { type: 'integer' }, after: { type: 'integer' }, buildOk: { type: 'boolean' }, newSim: { type: 'integer' }, gateUntampered: { type: 'boolean' }, notes: { type: 'string' } },
  required: ['after'],
}

async function safe(l, f) { try { return await f() } catch (e) { log('fail[' + l + ']: ' + ((e && e.message) || e)); return null } }

const RULES =
  `codefang Go->Rust port. The compat harness at ${COMPAT} is the HONEST scoreboard. Drive the REAL-divergence count down by FIXING THE PRODUCT, never the test.\n` +
  `ABSOLUTE RULES:\n` +
  `1. NEVER edit the harness/oracle/gate/canonicalizer/matrix/allowlist to make a cell pass. The integrity layer hashes them and fails closed; gaming = instant fail. A fix is valid ONLY if the LIVE Go-vs-Rust differential passes on off-golden inputs.\n` +
  `2. NEVER hardcode golden bytes or emit constants — the metamorphic layer flags that SIM. Output must compute from input, vary with input, grow with --limit, be deterministic.\n` +
  `3. The oracle is the live Go binary ${GOBIN}/{codefang,uast}; Rust under test ${RUBIN}/{codefang,uast}. Compare with \`set -f; env ${ENV} <bin> <argv>\`, STDOUT only.\n` +
  `4. An analyzer's json/yaml/bin/compact are encodings of ONE report value — fix the report value via the real analyzer logic, then route through cf-gojson/cf-goyaml/cf-reportutil so all encodings follow. Port logic from ${GO}/internal/analyzers/<name> and ${GO}/internal/framework.\n` +
  `5. Work ONLY under ${RUST} (analyzer crates + pipeline; NOT ${COMPAT}). Do NOT run git. Return PROSE from implementers; JSON only from verifiers.\n` +
  `6. Verify a cluster with: \`bash ${RUN} smoke\` then read ${SMOKE} (or filter the gate to the area). The cluster is done when its cells move FAIL->PASS with no new SIM and the gate confirms it.\n`

// run gate + cluster failures
async function measure(labelPhase) {
  return await safe('measure', () => agent(
    `${RULES}\n\nRun \`bash ${RUN} smoke 2>&1 | tail -20\`, then read ${SMOKE} and ${COMPAT}/results/smoke.json. Cluster the FAIL+SIM records by analyzer area (e.g. uast/parse, static/complexity, history/devs, sets) with their formats and a few sample labels. Return JSON: realDivergences, pass, sim, clusters[{area,count,formats,sampleLabels}], notes.`,
    { label: 'measure', phase: labelPhase, schema: CLUSTERS }))
}

// fix one area with bounded rounds, gate-verified
async function fixArea(area, prompt, rounds, phaseName) {
  let last = ''
  for (let r = 0; r < rounds; r++) {
    await safe('impl:' + area + ':' + r, () => agent(
      `${RULES}\n\n=== FIX CLUSTER: ${area} ===\n${prompt}\n` +
      (last ? `\nStill failing after last attempt:\n${last}\n` : '') +
      `Rebuild \`cargo build --release\`, then verify via the harness (\`bash ${RUN} smoke\` + read ${SMOKE}, or run the area's cells through ${COMPAT}/oracle/oracle.py). Return SHORT PROSE.`,
      { label: 'impl:' + area, phase: phaseName }))
    const v = await safe('verify:' + area + ':' + r, () => agent(
      `${RULES}\n\nVerify cluster ${area}: \`cargo build --release 2>&1 | tail -3\`, then \`bash ${RUN} smoke 2>&1 | tail -20\` and read ${SMOKE}. Count this area's remaining FAIL cells (after), confirm no NEW Sim, and confirm the harness files are UNMODIFIED (\`git status --porcelain ${COMPAT}\` style — if the harness changed, gateUntampered=false, that's a violation). Return JSON: area, after (remaining FAIL in this area), buildOk, newSim, gateUntampered, notes.`,
      { label: 'verify:' + area, phase: phaseName, schema: FIXV }))
    if (v && v.after === 0 && v.buildOk !== false && v.gateUntampered !== false && (v.newSim || 0) === 0) { log(area + ' CLEARED (round ' + r + ')'); return v }
    if (v && v.gateUntampered === false) { log(area + ' VIOLATION: harness modified — rejecting'); }
    last = v ? ('after=' + v.after + ' newSim=' + v.newSim + ' tampered=' + (v.gateUntampered === false) + '\n' + (v.notes || '')) : '(verify failed)'
    log(area + ' round ' + r + ': after=' + (v ? v.after : '?'))
  }
  return null
}

// =====================================================================
phase('Baseline')
const base = await measure('Baseline')
log('Baseline real divergences: ' + (base ? base.realDivergences : '?') + ' (pass=' + (base ? base.pass : '?') + ')')

// =====================================================================
// Grammars — biggest single cluster (22 uast/parse cells).
phase('Grammars')
await fixArea('uast/parse-grammars',
  `The Rust uast binary lacks 11 tree-sitter grammars (python, c, c-header, rust, typescript, tsx, javascript, json, yaml, cpp, shell) — it errors where Go emits a UAST, failing all uast/parse[json]+[compact] cells for those languages. Vendor those grammar crates into cf-uast (add the tree-sitter-<lang> crates, register them in the loader/parser language dispatch like the existing Go grammar), so \`uast parse --format json <file.py>\` etc. produce a UAST byte-identical to Go. Pin grammar versions to match the Go go-sitter-forest grammars that produced Go's output (check ${GO}/go.mod versions). Verify EACH language's uast/parse cell goes PASS via the live oracle.`,
  8, 'Grammars')
const afterGrammars = await measure('Grammars')
log('After grammars: ' + (afterGrammars ? afterGrammars.realDivergences : '?') + ' divergences')

// =====================================================================
// Static analyzers — fix report value once, all formats follow.
phase('Static')
const STATIC = ['static/complexity', 'static/comments', 'static/cohesion', 'static/composition', 'static/clones', 'static/halstead', 'static/imports']
await parallel(STATIC.map(a => () =>
  fixArea(a,
    `Static analyzer ${a} diverges from Go on off-golden dirs across formats (json/yaml/bin/compact). Port/fix the REAL static-analysis logic (UAST parse each file -> ${a} metric computation -> serialize) from ${GO}/internal/analyzers/${a.split('/')[1]} so output matches Go byte-for-byte on arbitrary directories (the harness tests dirs the golden never used). Fix the report VALUE once; all four encodings then follow via cf-gojson/cf-goyaml/cf-reportutil. Do not special-case the golden dir.`,
    5, 'Static')
))
const afterStatic = await measure('Static')
log('After static: ' + (afterStatic ? afterStatic.realDivergences : '?') + ' divergences')

// =====================================================================
// History analyzers — over the general pipeline, across formats.
phase('History')
const HISTORY = ['history/devs', 'history/imports', 'history/typos', 'history/burndown', 'history/anomaly', 'history/quality', 'history/sentiment', 'history/couples', 'history/shotness', 'history/file-history']
await parallel(HISTORY.map(a => () =>
  fixArea(a,
    `History analyzer ${a} diverges from Go on off-golden inputs (other dirs/limits/formats) — the broader matrix exposes gaps the 7 golden captures did not. Drive its REAL per-commit computation (git revwalk -> per-commit -> aggregate -> ComputeAllMetrics -> serialize) to match Go byte-for-byte across json/yaml/bin/compact for arbitrary --limit. Port from ${GO}/internal/analyzers/${a.split('/')[1].replace('-','_')}. Remember the typos-style trap: Go-STABLE fields (e.g. commit attribution) must match exactly — measure with the oracle, never assume nondeterminism.`,
    5, 'History')
))
const afterHistory = await measure('History')
log('After history: ' + (afterHistory ? afterHistory.realDivergences : '?') + ' divergences')

// =====================================================================
// Sets — multi-analyzer and --analyzers '*'.
phase('Sets')
await fixArea('analyzer-sets',
  `The multi-analyzer / --analyzers '*' cells (static/*, history/*, comma-lists, @hercules repo) diverge. Make the general dispatch run the requested SET of analyzers over one pass and emit the combined report matching Go (note all_static.* may be Go-nondeterministic — if so the oracle will measure it and the allowlist needs Go-nondeterminism EVIDENCE, which you may add to ${COMPAT}/allowlist.json ONLY with stored proof from running Go N times; that is the one sanctioned harness edit and only with evidence). Fix the genuinely-deterministic set cells to PASS.`,
  5, 'Sets')

// =====================================================================
phase('Regate')
const finalMeasure = await measure('Regate')
const selftest = await safe('selftest', () => agent(
  `Run the meta-gate \`bash ${SELFTEST} 2>&1 | tail -8\`. It MUST still prove the system catches a planted product bug AND fails closed on a harness cheat (this guarantees our fixes did not break the detector). Then run \`bash ${RUN} smoke 2>&1 | tail -6\` for the final tally. Return JSON: realDivergences, pass, sim, clusters[], notes (include whether the mutation self-test still passes).`,
  { label: 'regate', phase: 'Regate', schema: CLUSTERS }))
log('FINAL: divergences=' + (finalMeasure ? finalMeasure.realDivergences : '?') + ' selftest=' + (selftest ? selftest.notes : '?'))

return {
  baselineDivergences: base ? base.realDivergences : null,
  afterGrammars: afterGrammars ? afterGrammars.realDivergences : null,
  afterStatic: afterStatic ? afterStatic.realDivergences : null,
  afterHistory: afterHistory ? afterHistory.realDivergences : null,
  finalDivergences: finalMeasure ? finalMeasure.realDivergences : null,
  finalPass: finalMeasure ? finalMeasure.pass : null,
  finalSim: finalMeasure ? finalMeasure.sim : null,
  metaGateStillCatchesBugs: selftest ? (selftest.notes || '').toLowerCase().includes('pass') : null,
  nextStep: (finalMeasure && finalMeasure.realDivergences === 0)
    ? 'compat smoke GREEN with 0 real divergences and the meta-gate still proves bug-detection. Run `compat full` (486 cells) for the wider matrix.'
    : 'Resume: divergences remain — re-run this workflow; the harness gate shows exactly which clusters are left.',
}

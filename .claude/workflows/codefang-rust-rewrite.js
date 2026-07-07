export const meta = {
  name: 'codefang-rust-rewrite',
  description: 'Port codefang (Go) to Rust with byte-identical reports on ~/sources/kubernetes; keep libgit2',
  phases: [
    { title: 'Discover', detail: 'Map CLI, reports, analyzers, git2go, tree-sitter, config, build, dep-graph' },
    { title: 'Synthesize', detail: 'Merge discovery into ARCHITECTURE.md + topological port order' },
    { title: 'Golden', detail: 'Build Go binary; capture byte-exact reference reports on kubernetes' },
    { title: 'Design', detail: 'Architecture panel + dependency map + byte-identity strategy -> DESIGN.md' },
    { title: 'Scaffold', detail: 'Cargo workspace, identical clap CLI, git2, tree-sitter, golden-diff harness' },
    { title: 'Port', detail: 'Port ALL modules in dependency order (full autonomous); each cargo-verified' },
    { title: 'Verify', detail: 'Run Rust on kubernetes; byte-diff machine formats vs golden; fix loop' },
    { title: 'Review', detail: 'Completeness critic + adversarial correctness + resumable ROADMAP' },
  ],
}

// ---------- Constants ----------
const GO = '/home/dmitriy/sources/codefang'
const KUBE = '/home/dmitriy/sources/kubernetes'
const RUST = GO + '/rust'
const GOLDEN = GO + '/tests/golden'
const DOCS = GO + '/specs/rust-rewrite'
// FULL AUTONOMOUS ATTEMPT: port every module, no cap (long tail captured in ROADMAP for resume).
const PORT_LIMIT = Infinity
// BYTE-IDENTITY TARGET = MACHINE formats only (deterministic, diffable). Human-facing
// formats are captured but treated as best-effort (cosmetic diffs do not fail the run).
const MACHINE_FORMATS = ['json', 'yaml', 'ndjson', 'timeseries', 'timeseries+ndjson', 'compact', 'bin']
const HUMAN_FORMATS = ['text', 'plot', 'html']
const isMachine = (f) => MACHINE_FORMATS.includes(String(f || '').toLowerCase())

// ---------- Schemas ----------
const STR_LIST = { type: 'array', items: { type: 'string' } }
const DISCOVERY_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: {
    area: { type: 'string' },
    summary: { type: 'string' },
    files: STR_LIST,
    findings: { type: 'array', items: {
      type: 'object', additionalProperties: true,
      properties: { name: { type: 'string' }, detail: { type: 'string' }, path: { type: 'string' } },
      required: ['name', 'detail'],
    } },
    byteIdentityRisks: STR_LIST,
  },
  required: ['area', 'summary', 'findings'],
}
const PORTORDER_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: {
    docPath: { type: 'string' },
    modules: { type: 'array', items: {
      type: 'object', additionalProperties: true,
      properties: {
        name: { type: 'string' }, goPath: { type: 'string' }, crate: { type: 'string' },
        tier: { type: 'integer' }, deps: STR_LIST, purpose: { type: 'string' }, loc: { type: 'integer' },
      },
      required: ['name', 'goPath', 'crate', 'tier'],
    } },
  },
  required: ['modules'],
}
const GOLDEN_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: {
    built: { type: 'boolean' }, binaryPath: { type: 'string' },
    captures: { type: 'array', items: {
      type: 'object', additionalProperties: true,
      properties: {
        command: { type: 'string' }, argv: STR_LIST, format: { type: 'string' },
        outPath: { type: 'string' }, sha256: { type: 'string' }, bytes: { type: 'integer' }, ok: { type: 'boolean' },
        machine: { type: 'boolean' }, nonBinding: { type: 'boolean' },
      },
      required: ['command', 'outPath', 'ok'],
    } },
    notes: { type: 'string' },
  },
  required: ['built', 'captures'],
}
const SCORE_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: { score: { type: 'integer' }, rationale: { type: 'string' }, bestIdeas: STR_LIST, risks: STR_LIST },
  required: ['score', 'rationale'],
}
const DESIGN_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: { docPath: { type: 'string' }, summary: { type: 'string' }, depMapping: { type: 'array', items: {
    type: 'object', additionalProperties: true,
    properties: { go: { type: 'string' }, rust: { type: 'string' }, note: { type: 'string' } }, required: ['go', 'rust'],
  } }, byteIdentityStrategy: STR_LIST },
  required: ['docPath', 'summary'],
}
const SCAFFOLD_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: { ok: { type: 'boolean' }, createdFiles: STR_LIST, cargoBuilds: { type: 'boolean' }, cliMatches: { type: 'boolean' }, notes: { type: 'string' } },
  required: ['ok'],
}
const PORT_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: { module: { type: 'string' }, files: STR_LIST, externalCrates: STR_LIST, compiles: { type: 'boolean' }, testsPass: { type: 'boolean' }, todos: STR_LIST, notes: { type: 'string' } },
  required: ['module', 'compiles'],
}
const VERIFY_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: { rustBuilds: { type: 'boolean' }, diffs: { type: 'array', items: {
    type: 'object', additionalProperties: true,
    properties: { command: { type: 'string' }, identical: { type: 'boolean' }, binding: { type: 'boolean' }, firstDiff: { type: 'string' }, bytesGo: { type: 'integer' }, bytesRust: { type: 'integer' } }, required: ['command', 'identical'],
  } }, identicalCount: { type: 'integer' }, totalCount: { type: 'integer' }, notes: { type: 'string' } },
  required: ['rustBuilds', 'diffs'],
}
const REVIEW_SCHEMA = {
  type: 'object', additionalProperties: true,
  properties: { remainingWork: STR_LIST, correctnessFindings: STR_LIST, roadmapPath: { type: 'string' }, percentComplete: { type: 'integer' } },
  required: ['remainingWork'],
}

async function safe(label, fn) {
  try { return await fn() } catch (e) { log('step failed [' + label + ']: ' + ((e && e.message) || e)); return null }
}

// =====================================================================
// PHASE 1 — DISCOVER (parallel readers; barrier to synthesize)
// =====================================================================
phase('Discover')
const AREAS = [
  { area: 'cli', prompt: `Map the COMPLETE CLI interface of the Go project at ${GO}. Read everything under cmd/ and any cobra command files. Report: the binary name; the full command/subcommand tree; for every command, EVERY flag (long+short name, type, default value, help text, whether persistent); positional args; exit codes; and how stdout vs stderr is used. The Rust rewrite must reproduce this interface byte-for-byte (clap). Read .codefang.yaml and report config keys too.` },
  { area: 'reports', prompt: `Find ALL report/output generation in the Go project at ${GO}. Search pkg/ and internal/ for report, render, format, output, writer, serialize, marshal, encode, table, chart code. Report: every output FORMAT (json, yaml, terminal table, html/echarts, markdown, csv); the exact serialization path for each; what controls field ORDER, map-key SORTING, FLOAT formatting (fmt verbs / strconv), integer/percentage/byte-size humanization, indentation, HTML escaping, trailing newlines. List concrete file:func sites. Flag every place where Go's encoding/json (struct field order, SetEscapeHTML), gopkg.in/yaml.v3, go-pretty/jedib0t, or go-humanize formatting could differ from a naive Rust port — these are byteIdentityRisks.` },
  { area: 'analyzers', prompt: `Inventory ALL analyzers in the Go project at ${GO}. Find the Analyzer interface and every implementation. For each: package path, Name(), what it computes, inputs/outputs, and any nondeterminism (map iteration, goroutine ordering, randomness, time). Cover technical-debt, churn/developer, AST/structure, git-history, language-stats, sentiment analyzers. List file:func.` },
  { area: 'git', prompt: `Document EVERY use of libgit2 via github.com/libgit2/git2go/v34 in ${GO}. Grep for git2go imports and list each call site with the git operation performed (open repo, walk, log, blame, diff, tree, commit, object lookup, etc.) and the surrounding function. The Rust rewrite KEEPS libgit2 through the git2 crate — note any git2go API that maps awkwardly to the git2 crate.` },
  { area: 'parsing', prompt: `Document tree-sitter usage in ${GO}: the go-sitter-forest grammars and go-tree-sitter-bare. How are parsers created, which languages, how is the AST walked/queried, what queries/captures are used? List the parsing abstraction (interfaces, registry). The Rust port uses the official tree-sitter crates + per-language grammar crates — list which languages must be supported.` },
  { area: 'deps', prompt: `For the Go project at ${GO}, explain how each non-trivial dependency is USED and where: enry (src-d/enry) language detection, govader sentiment, go-echarts charts, go-pretty tables, go-humanize, pierrec/lz4, prometheus/client_golang + go.opentelemetry.io/otel telemetry, tliron/glsp (LSP server?), spf13/cobra+viper, xeipuuv/gojsonschema, sergi/go-diff, fatih/color. For each, give the call sites and what a byte-identical Rust replacement must reproduce (esp. enry classification results and govader lexicon/scoring, which affect report bytes).` },
  { area: 'build', prompt: `Read the Makefile, Dockerfile, .goreleaser.yml, and go.mod at ${GO}. Report: exact build command and CGO flags for libgit2 (CGO_CFLAGS/LDFLAGS/PKG_CONFIG_PATH), how the binary is produced and named, where output goes (build/?), how tests/lint/deadcode/bench run, and how libgit2 itself is provided (third_party/libgit2 submodule? system pkg-config?). The Rust build must link the same libgit2.` },
  { area: 'depgraph', prompt: `Produce the internal PACKAGE DEPENDENCY GRAPH for ${GO} (cmd/, pkg/, internal/). For each package: import path, one-line purpose, non-test LOC, and which other internal packages it imports. Identify the layering (domain/adapters/frameworks) and a TOPOLOGICAL order from leaf (no internal deps) to root (cmd). This order drives the Rust port sequence. In findings, list each package as a finding with name=import path, detail=purpose+deps+LOC, path=dir.` },
]
const discovery = (await parallel(AREAS.map(a => () =>
  agent(a.prompt + ' Return structured JSON per the schema. Be exhaustive and concrete with file paths.', { label: 'discover:' + a.area, phase: 'Discover', schema: DISCOVERY_SCHEMA })
))).filter(Boolean)
log('Discovery complete: ' + discovery.length + '/' + AREAS.length + ' areas mapped')

// =====================================================================
// PHASE 2 — SYNTHESIZE architecture + topological port order
// =====================================================================
phase('Synthesize')
const portOrder = await safe('synthesize', () => agent(
  `You are the lead architect for porting codefang (Go) to Rust. Below is structured discovery output from 8 parallel investigators.\n\n` +
  JSON.stringify(discovery) +
  `\n\nDo two things:\n` +
  `1. WRITE a comprehensive architecture map to ${DOCS}/ARCHITECTURE.md (create dirs as needed). Cover: binary/CLI tree with every flag, report formats and their exact serialization rules, analyzer inventory, git2go usage, tree-sitter usage, dependency usage, build/CGO/libgit2, and the internal package layering. Include the consolidated byte-identity risk list.\n` +
  `2. Decide the TOPOLOGICAL PORT ORDER: list every internal package as a module with name, goPath, proposed Rust crate name, tier (0=leaf no internal deps, increasing toward cmd), internal deps, purpose, and LOC. Tier 0/1 are the foundation to port first.\n` +
  `Return the module list as JSON (and docPath=the written file).`,
  { label: 'synthesize:architecture', phase: 'Synthesize', schema: PORTORDER_SCHEMA }
))
const modules = (portOrder && portOrder.modules) || []
log('Architecture written; ' + modules.length + ' modules ordered for porting')

// =====================================================================
// PHASE 3 — GOLDEN capture (sequential, side effects)
// =====================================================================
phase('Golden')
const golden = await safe('golden', () => agent(
  `Establish BYTE-EXACT golden reference outputs for the Go codefang tool, to validate the Rust rewrite.\n` +
  `Steps:\n` +
  `1. Build the Go binary in ${GO} (use the Makefile target; honor CGO/libgit2 flags from this build info: ${JSON.stringify((discovery.find(d => d.area === 'build') || {}))}).\n` +
  `2. From the CLI inventory: ${JSON.stringify((discovery.find(d => d.area === 'cli') || {}))} and report inventory: ${JSON.stringify((discovery.find(d => d.area === 'reports') || {}))}, enumerate EVERY report-producing command in EVERY output format.\n` +
  `   BYTE-IDENTITY TARGET IS MACHINE FORMATS ONLY: ${MACHINE_FORMATS.join(', ')} (these are the binding goldens that the Rust port must match byte-for-byte). Human formats (${HUMAN_FORMATS.join(', ')}) should still be captured but marked nonBinding=true — cosmetic differences are acceptable for them.\n` +
  `   The primary commands to cover exhaustively: 'codefang run <path> --format <fmt>' for every machine format and representative analyzer selections (single analyzer, all analyzers), and 'uast parse/analyze/query --format json'. Bin output: capture the raw bytes AND a stable hex/sha so the Rust harness can compare.\n` +
  `3. Run each against the repo at ${KUBE}. Capture stdout EXACTLY to files under ${GOLDEN}/ (create dirs). Use a stable filename per (command,format). Record the exact argv used, byte length, and sha256 of each output. Set deterministic env (TZ=UTC, NO_COLOR=1, LANG=C, LC_ALL=C, SOURCE_DATE_EPOCH if honored) AND record exactly which env you set so the Rust harness can match it. Pin nondeterminism: prefer flags that bound work (e.g. --limit, --head, --workers 1) to make output reproducible across runs; VERIFY each binding golden is stable by running it twice and confirming identical sha256. If a command cannot be made deterministic, mark nonBinding=true and note why.\n` +
  `4. Also write ${GOLDEN}/MANIFEST.json describing every capture (command, argv, env, format, outPath, sha256, bytes, machine boolean, nonBinding boolean).\n` +
  `Be careful: ${KUBE} is large — if a full run is too slow, ALSO capture a fast deterministic subset (e.g. a few subdirectories) and record both. Return the capture manifest as JSON.`,
  { label: 'golden:capture', phase: 'Golden', schema: GOLDEN_SCHEMA }
))
log('Golden capture: ' + (golden ? golden.captures.filter(c => c.ok).length + ' reports captured' : 'FAILED — Go build or run blocked; verify will be limited'))

// =====================================================================
// PHASE 4 — DESIGN (judge panel -> synthesis)
// =====================================================================
phase('Design')
const ctxForDesign = { build: discovery.find(d => d.area === 'build'), reports: discovery.find(d => d.area === 'reports'), deps: discovery.find(d => d.area === 'deps'), git: discovery.find(d => d.area === 'git'), parsing: discovery.find(d => d.area === 'parsing'), modules }
const ANGLES = [
  { key: 'identity-first', lens: 'Optimize ABOVE ALL for byte-identical MACHINE output (json, yaml, ndjson, timeseries, compact, bin) — terminal/HTML are cosmetic best-effort. Make the Rust serialization layer mirror Go encoding/json EXACTLY: struct field DECLARATION order (NOT alphabetical), map keys sorted by Go string-byte order, HTML escaping of <,>,& on by default (json.Encoder), shortest-round-trip float formatting matching strconv.AppendFloat(\'g\',-1) — note serde_json/ryu differ from Go on exponent thresholds and integer-valued floats, so a custom Go-compatible float formatter is likely REQUIRED; integers as-is; RFC3339/RFC3339Nano time strings. For YAML, reproduce gopkg.in/yaml.v3 output (indent width, string quoting, key order via yaml.Node). For the LZ4 bin format, match the exact frame/record layout and compression settings. Propose a custom serialization compat crate rather than relying on serde defaults.' },
  { key: 'idiomatic-rust', lens: 'Optimize for clean idiomatic Rust (workspace of crates, serde, thiserror/anyhow, rayon) while still guaranteeing byte-identity via a thin formatting-compat module. Favor maintainability.' },
  { key: 'risk-first', lens: 'Optimize for de-risking the hardest byte-identity threats first: enry language classification parity, govader sentiment lexicon/scoring parity, float/percentage/humanize formatting, and map ordering nondeterminism. Propose how to port or vendor the exact data tables and how to test each in isolation.' },
]
const proposals = (await parallel(ANGLES.map(a => () =>
  agent(`Propose a complete Rust architecture for porting codefang. LENS: ${a.lens}\n\nContext (discovery + module list):\n${JSON.stringify(ctxForDesign)}\n\nCover: Cargo workspace/crate layout, dependency mapping table (Go dep -> Rust crate, keep libgit2 via git2), the CLI compat plan (clap reproducing cobra exactly), the byte-identity serialization strategy in concrete terms, the tree-sitter language set, and the test harness that byte-diffs against golden. Return prose.`,
    { label: 'design:' + a.key, phase: 'Design' })
))).filter(Boolean)
const judged = await parallel(proposals.map((p, i) => () =>
  agent(`Score this Rust-architecture proposal (0-100) for codefang, weighting byte-identical-output feasibility highest, then correctness, then maintainability. Proposal:\n\n${p}`,
    { label: 'judge:' + ANGLES[i].key, phase: 'Design', schema: SCORE_SCHEMA })
))
const ranked = proposals.map((p, i) => ({ p, key: ANGLES[i].key, s: (judged[i] && judged[i].score) || 0, ideas: (judged[i] && judged[i].bestIdeas) || [] }))
  .sort((a, b) => b.s - a.s)
const design = await safe('design-synth', () => agent(
  `Synthesize the FINAL Rust rewrite design for codefang. Base it on the winning proposal and graft the best ideas from the others.\n\n` +
  `WINNER (${ranked[0] ? ranked[0].key : 'n/a'}):\n${ranked[0] ? ranked[0].p : ''}\n\n` +
  `RUNNER-UP IDEAS:\n${JSON.stringify(ranked.slice(1).map(r => ({ key: r.key, ideas: r.ideas })))}\n\n` +
  `Golden manifest (what must match byte-for-byte): ${JSON.stringify(golden ? golden.captures : [])}\n\n` +
  `SCOPE: byte-identity is REQUIRED only for MACHINE formats (${MACHINE_FORMATS.join(', ')}); terminal/HTML (${HUMAN_FORMATS.join(', ')}) are best-effort/cosmetic. Confirmed dep swaps: keep libgit2 via the git2 crate; tree-sitter official crates + per-language grammar crates; clap for the CLI; idiomatic Rust elsewhere (serde+custom Go-compat encoder, comfy-table/tabled for tables, a charts crate or templated HTML for plots, etc.).\n` +
  `WRITE the final design to ${DOCS}/DESIGN.md including: (a) Cargo workspace + crate-per-module layout aligned to the port order; (b) full dependency-mapping table keeping libgit2 via the git2 crate; (c) the precise BYTE-IDENTITY strategy for MACHINE formats — a dedicated go-compat serialization crate that reproduces Go encoding/json (declaration-order fields, byte-sorted map keys, HTML-escape, Go-compatible shortest float formatter), gopkg.in/yaml.v3 output, the LZ4 bin record layout, plus enry classification parity and govader lexicon/scoring parity (these change report bytes); explicitly mark go-pretty tables / go-echarts HTML as non-binding; (d) the clap CLI compat plan reproducing every cobra command, flag (long+short), default, and help text for BOTH the codefang and uast binaries; (e) the golden-diff integration-test harness design that diffs only the binding (machine) goldens and reports human-format diffs as informational. Return JSON with docPath, summary, depMapping, byteIdentityStrategy.`,
  { label: 'design:synthesize', phase: 'Design', schema: DESIGN_SCHEMA }
))
log('Design written: ' + (design ? design.docPath : 'FAILED'))

// =====================================================================
// PHASE 5 — SCAFFOLD (sequential; persistent files in real tree)
// =====================================================================
phase('Scaffold')
const scaffold = await safe('scaffold', () => agent(
  `Scaffold the Rust rewrite of codefang under ${RUST} (Cargo workspace) on the current git branch. Use the design at ${DOCS}/DESIGN.md and this summary: ${JSON.stringify(design || {})}.\n` +
  `Do:\n` +
  `1. Create the Cargo workspace (workspace Cargo.toml + crate skeletons matching the planned crate layout). Pin edition 2021+, set up the workspace members.\n` +
  `2. Create the CLI crate that reproduces codefang's cobra interface EXACTLY using clap (every command, flag long/short, default, help text). Wire git2 as a dependency and confirm it links against the same libgit2 used by the Go build. Add the tree-sitter crates for the required languages (feature-gated is fine).\n` +
  `3. Implement \`--help\`/\`--version\` and the command dispatch so the binary runs and prints help. Subcommand bodies may be stubbed with explicit unimplemented! ONLY where not yet ported, but the CLI surface must be complete.\n` +
  `4. Create the golden-diff integration test harness under ${RUST}/tests/ that: builds the rust binary, runs each invocation from ${GOLDEN}/MANIFEST.json with the same env, and byte-compares stdout against the golden file, reporting the first differing byte offset. Wire it so it currently SKIPs (not fails) commands whose implementation is still stubbed, but runs for implemented ones.\n` +
  `5. Run \`cargo build\` and \`cargo test --no-run\` and report whether it compiles. Run the rust binary's --help and diff its structure against the Go binary's --help.\n` +
  `Return JSON: ok, createdFiles, cargoBuilds, cliMatches, notes. Do NOT delete any Go files.`,
  { label: 'scaffold:workspace', phase: 'Scaffold', schema: SCAFFOLD_SCHEMA }
))
log('Scaffold: ' + (scaffold && scaffold.ok ? 'created; builds=' + scaffold.cargoBuilds + ' cliMatches=' + scaffold.cliMatches : 'FAILED'))

// =====================================================================
// PHASE 6 — PORT foundation/leaf modules (pipeline: port -> verify)
// =====================================================================
phase('Port')
const portTargets = modules
  .slice()
  .sort((a, b) => (a.tier - b.tier) || ((a.loc || 0) - (b.loc || 0)))
  .slice(0, PORT_LIMIT)
log('FULL AUTONOMOUS PORT: attempting all ' + portTargets.length + ' modules in dependency order (tier asc, then LOC asc)')
const ported = await pipeline(
  portTargets,
  (m) => agent(
    `Port the Go package "${m.name}" (${m.goPath}, purpose: ${m.purpose || 'n/a'}) to its Rust crate "${m.crate}" under ${RUST}, following ${DOCS}/DESIGN.md.\n` +
    `Rules: (1) Reproduce behavior exactly. Byte-identity of MACHINE-format report bytes (${MACHINE_FORMATS.join(', ')}) is the project goal; route all report serialization through the shared go-compat serialization crate from the design rather than raw serde. (2) Use the dependency mapping from the design (libgit2 via git2, tree-sitter crates, clap). (3) Write full implementations with rustdoc and unit tests ported from the Go tests where they exist. (4) Create/edit ONLY files under your crate's directory — do NOT edit the workspace Cargo.toml; instead return the list of external crates your crate needs in externalCrates so they can be integrated centrally. (5) Where a transitive dep on a not-yet-ported module blocks you, define the minimal trait/interface and note it in todos. (6) For GENERATED or large embedded-data modules (e.g. embedded UAST mappings / uastmaps / *.gen.go), do NOT hand-translate the generated artifact — port the GENERATOR and/or add a build.rs (or a tools step) that regenerates the equivalent Rust data, and note this in todos. (7) For data-parity-critical modules (enry language data, govader lexicons), vendor the SAME data tables the Go libs use so classification/scoring match byte-for-byte.\n` +
    `Return JSON: module, files, externalCrates, compiles(self-assessed), testsPass, todos, notes.`,
    { label: 'port:' + m.name, phase: 'Port', schema: PORT_SCHEMA }),
  (res, m) => agent(
    `In ${RUST}, integrate the just-ported crate "${m.crate}" (module ${m.name}): add any missing external crates (${JSON.stringify(res ? res.externalCrates : [])}) to the workspace/crate Cargo.toml, then run \`cargo build -p ${m.crate}\` and \`cargo test -p ${m.crate}\`. Fix compile errors you can resolve quickly without changing behavior. Report whether it compiles and tests pass.\n` +
    `Return JSON: module="${m.name}", files, externalCrates, compiles, testsPass, todos, notes.`,
    { label: 'verify-port:' + m.name, phase: 'Port', schema: PORT_SCHEMA })
)
const portedOk = ported.filter(Boolean)
log('Ported: ' + portedOk.filter(p => p.compiles).length + '/' + portTargets.length + ' modules compile')

// =====================================================================
// PHASE 7 — VERIFY byte-identity (bounded fix loop)
// =====================================================================
phase('Verify')
let verify = await safe('verify', () => agent(
  `Verify byte-identity of the Rust rewrite under ${RUST} against the golden outputs in ${GOLDEN} (manifest ${GOLDEN}/MANIFEST.json).\n` +
  `1. \`cargo build --release\` (or debug if release fails). 2. For each BINDING capture (machine=true / nonBinding!=true: ${MACHINE_FORMATS.join(', ')}) whose command is implemented, run the Rust binary with the SAME argv+env and byte-compare stdout to the golden file. 3. ALSO run human-format (text/plot/html) captures but report those diffs as INFORMATIONAL only — they do NOT count as failures. 4. Report per-command: identical?, binding?, first differing byte offset + a short hexdump-style context, and byte counts.\n` +
  `Return JSON: rustBuilds, diffs[], identicalCount (binding only), totalCount (binding only), notes. It is EXPECTED that many commands are still stubbed this run — only compare implemented ones and say so.`,
  { label: 'verify:diff', phase: 'Verify', schema: VERIFY_SCHEMA }
))
let round = 0
const MAX_VERIFY_ROUNDS = 5
while (verify && verify.rustBuilds && round < MAX_VERIFY_ROUNDS) {
  // Only BINDING (machine) format diffs must be driven to zero.
  const failing = (verify.diffs || []).filter(d => !d.identical && d.binding !== false)
  if (!failing.length) { log('Verify: all implemented BINDING (machine-format) commands are byte-identical'); break }
  log('Verify round ' + round + ': ' + failing.length + ' binding commands differ — spawning fixers')
  await parallel(failing.slice(0, 12).map(d => () =>
    agent(`A Rust MACHINE-format report does not byte-match the Go golden. Command: ${d.command}. First diff: ${d.firstDiff || '(see golden vs rust)'}. Golden dir: ${GOLDEN}. Rust source: ${RUST}.\n` +
      `Find the exact formatting/logic discrepancy (likely: encoding/json field-declaration order vs alphabetical, map-key byte-sort, Go shortest-float formatting vs ryu, integer-valued-float rendering, HTML escaping of <>&, indentation, trailing newline, yaml.v3 quoting/indent, LZ4 bin layout, enry classification parity, govader lexicon/scoring parity) and patch the Rust code so the output matches byte-for-byte. Prefer fixing the shared go-compat serialization crate when the bug is systemic. Re-run the single command and confirm identical. Report what you changed.`,
      { label: 'fix:' + d.command, phase: 'Verify' })
  ))
  round++
  verify = await safe('verify-recheck', () => agent(
    `Re-run the byte-identity check for the Rust rewrite under ${RUST} against golden ${GOLDEN}/MANIFEST.json. Compare BINDING (machine) captures for failures; report human-format diffs as informational. Return JSON: rustBuilds, diffs[] (each with binding boolean), identicalCount, totalCount, notes.`,
    { label: 'verify:recheck-' + round, phase: 'Verify', schema: VERIFY_SCHEMA }))
}

// =====================================================================
// PHASE 8 — REVIEW: completeness critic + adversarial correctness + roadmap
// =====================================================================
phase('Review')
const review = await safe('review', () => agent(
  `Final review of the codefang Rust rewrite first pass.\n` +
  `Inputs: modules total=${modules.length}, ported this run=${JSON.stringify(portedOk.map(p => p.module))}, verify result=${JSON.stringify(verify || {})}, golden=${JSON.stringify(golden ? golden.captures.map(c => c.command) : [])}.\n` +
  `1. COMPLETENESS CRITIC: list everything still missing for byte-identical parity — unported modules, stubbed commands, unverified report formats, and the top byte-identity risks (enry/govader parity, float/humanize formatting, map ordering, yaml quirks).\n` +
  `2. ADVERSARIAL CORRECTNESS: inspect the ported Rust crates under ${RUST} for behavior-diverging bugs vs the Go source; list concrete findings with file:line.\n` +
  `3. WRITE a resumable ROADMAP to ${DOCS}/ROADMAP.md as a checklist (unchecked items) covering the remaining port order and per-command byte-identity verification, so this workflow can be resumed module by module. Match the project's roadmap style (WHAT + DoD, no time estimates).\n` +
  `Return JSON: remainingWork, correctnessFindings, roadmapPath, percentComplete.`,
  { label: 'review:complete', phase: 'Review', schema: REVIEW_SCHEMA }
))
log('Review done; roadmap: ' + (review ? review.roadmapPath : 'n/a') + '; ~' + (review ? review.percentComplete : 0) + '% complete')

return {
  modulesTotal: modules.length,
  portedThisRun: portedOk.map(p => ({ module: p.module, compiles: p.compiles, testsPass: p.testsPass })),
  golden: golden ? { captured: golden.captures.filter(c => c.ok).length, manifest: GOLDEN + '/MANIFEST.json' } : null,
  byteIdentity: verify ? { identical: verify.identicalCount, total: verify.totalCount } : null,
  docs: { architecture: DOCS + '/ARCHITECTURE.md', design: DOCS + '/DESIGN.md', roadmap: DOCS + '/ROADMAP.md' },
  rustWorkspace: RUST,
  percentComplete: review ? review.percentComplete : null,
  nextStep: 'Resume this workflow to port the next tier of modules; rerun Verify to drive byte-identity to 100%.',
}

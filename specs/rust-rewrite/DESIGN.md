# Rust Rewrite Design — codefang → Rust, byte-identical machine output

Status: FINAL. This document is the synthesized, ground-truth-verified design for
porting codefang from Go to Rust. It supersedes prior drafts. The single overriding
constraint is **byte-for-byte identity of MACHINE report formats** against the Go
binary, validated by a golden-diff harness over a pinned corpus (`~/sources/kubernetes`).

## 0. Scope: what must be byte-identical, and what must not

Byte-identity is **REQUIRED** for these MACHINE formats (golden manifest, `nonBinding:false`):

| Format | Go encoder shape | Trailing `\n`? |
| --- | --- | --- |
| `json` | `json.MarshalIndent(v,"","  ")` (most analyzers, via `common/reporter.go`) **or** `json.NewEncoder(w).SetIndent("","  ").Encode` (`static.go`) — per call site | indent: no; encoder: yes |
| `compact` | `json.Marshal(v)` | no |
| `ndjson` | `json.NewEncoder(w).Encode(v)` per record | yes (per record) |
| `timeseries` | `json.NewEncoder(w)` (+ `SetIndent` per site) | yes |
| `timeseries+ndjson` | combo of the two streaming encoders | yes |
| `bin` | CFB1 envelope: `"CFB1"` + `u32 LE len` + raw `json.Marshal(payload)`, concatenated | n/a (binary) |
| `yaml` | `gopkg.in/yaml.v3` marshal | yaml.v3 default |

Byte-identity is **NOT required** (best-effort / cosmetic, `nonBinding:true` or `machine:false`):
`text` (go-pretty tables), `plot` / `html` (go-echarts templates), and any analyzer marked
**UNSTABLE map/pair/node order** in the golden manifest (`static/clones`, `static/cohesion`,
`static/comments` json, `history/couples`, `history/shotness`, `history/file-history`,
combined `static/*`). For UNSTABLE outputs we match Go's *logical* content and structure but
do **not** assert byte equality (Go itself is non-deterministic there due to map iteration order).

The golden manifest is the contract: 33 binding goldens (json/yaml/bin/timeseries/ndjson/
compact across burndown, anomaly, devs, imports, quality, sentiment, typos, complexity,
composition, comments, halstead, imports, and the `uast` binary's parse/analyze/query/count)
plus the CLI surface (`--help`, `version`, format-validation error strings) are byte-pinned.

## 0.1 Three facts that shape the whole design (verified against the tree)

1. **`bin` is CFB1, not LZ4.** `internal/analyzers/common/reportutil/binary.go`
   (`EncodeBinaryEnvelope`) writes `magic "CFB1"` (4 bytes) + `binary.LittleEndian.PutUint32(len)`
   + raw `json.Marshal(payload)`, concatenated per record, erroring when `len > math.MaxUint32`.
   LZ4 (`pierrec/lz4/v4`) and `encoding/gob` appear only in the on-disk **spill / file-report
   / checkpoint** stores, which never cross the Go↔Rust boundary as user output. Those get
   *logical* parity, not byte parity.

2. **Three distinct Go JSON call shapes are in play and they differ in bytes.** A single
   `marshal()` is wrong. `json.Marshal` (compact, no `\n`), `json.MarshalIndent(v,"","  ")`
   (2-space, no `\n`), and `json.NewEncoder(w).Encode` (HTML-escape on, **appends `\n` after
   every `Encode`**, optional `SetIndent("","  ")`). Verified: `common/reporter.go:77,226` use
   `MarshalIndent`; `static.go` and the timeseries/ndjson sinks use `NewEncoder`. The trailing
   newline is the single most common one-byte golden diff.

3. **The keystones already exist and are correct in spirit.** `rust/crates/cf-gojson`
   (`value.rs`, `marshal.rs`, `ftoa.rs`) already models declaration-order structs vs
   byte-sorted map keys, HTML escaping of `<>&`, and the two Go float encoders
   (`format_json_float` = json's `'g'` with ≥1-digit exponent and `1e-6`/`1e21` thresholds;
   `format_float_g` = `strconv`'s `'g'` with ≥2-digit exponent and `decExp<-4||>=21`). This
   design **ratifies and completes** that structure; it does not invent a parallel one.

---

## 1. Cargo workspace and crate-per-module layout (aligned to port order)

Single **virtual workspace** at `rust/` (already in place): `[workspace] resolver="2"`,
`members=["crates/*","bins/*","tests/golden-harness"]`, no root package. `[workspace.package]`
pins `version="0.0.0"` (so no version string leaks into output), `edition="2021"`,
`rust-version="1.80"`. One crate per Go package, named `cf-<module>`, mirroring `goPath`.
`crates/analyzers` is a nesting directory (excluded from the `crates/*` glob);
`cf-govader` is excluded from the default member glob and pulled in explicitly.

### 1.1 Tiering = port order (build bottom-up; each tier gates the next)

- **Tier -1 — serialization-compat keystones (do first, gate everything in the report path):**
  `cf-gojson` (Go `encoding/json` value model + 3 marshallers + float formatter — exists),
  `cf-goyaml` (yaml.v3 emitter — exists, needs body), `cf-reportutil` (CFB1 envelope).
  A thin format-dispatch facade (in `cf-analyze`/`cf-reportutil`) owns the mapping of the
  seven format names to the exact Go call shape so individual analyzers never pick an encoder.
- **Tier 0 — leaf keystones:** `cf-version`, `cf-safeconv`, `cf-units`, `cf-textutil`,
  the `cf-alg-*` leaves (`cf-alg-hashutil` first — seeds are byte-critical), `cf-uast-node`
  (serialized UAST node model — port and golden-test its JSON standalone early).
- **Tier 1–3 — UAST + plumbing:** `cf-uast-mapping` (PEG DSL for runtime input,
  plus the typed static mapping model + `uast_language!` macro), `cf-uast-spec`,
  `cf-uast-mappings` (the mapping SYSTEM OF RECORD: 68 Rust-native static
  tables, transpiled from the Go DSL corpus and equality-gated against the DSL
  parser — see specs/uastmap-rust-macros), `cf-uast-uastmaps` (the frozen
  `.uastmap` snapshot: dev-server text endpoints + gate input only; the
  analysis pipeline reads the static registry), `cf-uast`, `cf-gitlib`,
  `cf-analyzers-plumbing` (`FileDiff`/`TreeDiff`/`LinesStats`), `cf-pathfilter`,
  `cf-langpath` (enry parity), `cf-cache`.
- **Tier 4–6 — estimators + analyzers:** `cf-alg-hll/-cms/-minhash/-bloom/-lsh/-stats`,
  then analyzer crates (`cf-burndown-core`/`cf-analyzer-burndown`, `cf-anomaly`, `cf-devs`,
  `cf-couples`, `cf-imports`, `cf-quality`, `cf-sentiment`+`cf-govader`, `cf-typos`,
  `cf-complexity`, `cf-composition`, `cf-comments`, `cf-halstead`, `cf-clones`,
  `cf-cohesion`, `cf-shotness`, `cf-file-history`).
- **Tier 7–8 — framework/pipeline/CLI plumbing:** `cf-framework`, `cf-pipeline`,
  `cf-streaming`, `cf-storage`/`cf-persist`/`cf-spillstore`/`cf-checkpoint`,
  `cf-renderer`/`cf-terminal`/`cf-plotpage`, `cf-config`, `cf-commands`, `cf-identity`.
- **Tier 9 — binaries:** `bins/uast`, `bins/codefang`.

**CI invariant:** `serde_json`/`serde_yaml`/`ryu` and `chrono`'s RFC3339 are forbidden in any
non-test, non-build-script file in the report path (grep lint, §5). serde stays in
`[workspace.dependencies]` for the golden harness and build scripts only.

---

## 2. Dependency mapping (Go → Rust) — keeping libgit2 via git2

| Go dependency | Rust crate | Notes |
| --- | --- | --- |
| `encoding/json` | **`cf-gojson`** (custom) | byte-parity keystone; 3 marshallers; never serde_json in report path |
| `gopkg.in/yaml.v3` | **`cf-goyaml`** (custom) | byte-parity keystone; yaml.v3 emitter, not serde_yaml |
| `strconv.FormatFloat` | `cf-gojson::ftoa` | two `'g'` variants + f32 path |
| `time` RFC3339/Nano | `cf-gotime` helper (small; can fold into `cf-safeconv`) | fractional-trim parity; **not** chrono default RFC3339 |
| `encoding/binary` (CFB1) | `cf-reportutil::encode_binary_envelope` | magic+`u32::to_le_bytes`+`marshal` payload |
| `git2go` / libgit2 | **`git2` 0.19 + `vendored-libgit2`** | **KEEP libgit2**; `third_party/libgit2` submodule pins version; per-thread `!Send`/`!Sync` `Repository`; RAII `Drop` replaces `Free()`; diff options (context, rename detect, whitespace) set identically to git2go |
| `tree-sitter` (68 grammars) | `tree-sitter` 0.22 + per-language `tree-sitter-<lang>` crates behind cargo features | positions/types flow to output → pin grammar versions; bump = golden-breaking |
| `go-enry` | enry-data port / vendored table in `cf-langpath`/`cf-pathfilter`/`cf-composition` | **classification parity changes report bytes** (path filtering, `static/composition` language buckets) — golden-test against enry output |
| govader / VADER (`internal/analyzers/sentiment/lexicons`) | **`cf-govader`** + `cf-sentiment-lexicons` | **lexicon + scoring parity changes report bytes**; vendor lexicon tables verbatim; reproduce VADER scoring constants exactly; compute in **f32** like Go |
| `pierrec/lz4/v4` | `lz4_flex` (frame mode) | spill/test stores only — **not** user report bytes |
| `encoding/gob` | custom length-framed encoder | internal stores only — **logical** parity, not byte parity |
| cobra | **clap 4 builder API** (not derive) | reproduce command/flag order, help, errors (§4) |
| viper | `cf-config` hand-port | YAML+env+defaults precedence reproduced |
| go-pretty table | `cf-terminal` (comfy-table/tabled) | `text` format — **NON-BINDING** |
| go-echarts / HTML templates | `cf-plotpage` (charts crate or templated HTML) | `plot`/`html` — **NON-BINDING** |
| OpenTelemetry | `cf-observability` (tracing/opentelemetry) | no report bytes |
| `os/signal` | `signal-hook` in `cf-sigutil` | — |
| sha/hash seeds | `cf-alg-hashutil` hand-port, **identical seed constants** | byte-critical: HLL/CMS/MinHash/Bloom/LSH derive from these; any divergence shifts estimator outputs and report numbers |
| (new) parallelism | `rayon` | pipeline stages in `cf-framework` |

**Silent byte hazards** to golden-test in isolation before trusting any consumer:
(a) the probabilistic estimators — outputs depend on exact hash values, register indexing,
iteration order; (b) enry language classification; (c) govader lexicon/scoring.

---

## 3. Byte-identity strategy for MACHINE formats (concrete)

### 3.1 Value model — `cf-gojson::value`
`GoValue = Null | Bool | Int(i64) | Uint(u64) | Float(f64) | Str(String) | Array(Vec<GoValue>) | Map(GoMap)`.
`GoMap` carries a `MapOrigin` discriminant — **the heart of Go JSON parity**:
- `MapOrigin::Struct` → keys retain **declaration (insertion) order**, never sorted
  (reproduces Go reflect-over-struct-fields). Each report type is a
  `fn to_govalue(&self) -> GoValue` that pushes fields **in the exact order the Go struct
  declares them**.
- `MapOrigin::Map` → keys **sorted by Go string-byte comparison** at encode time
  (raw `[u8]` lexicographic = Go `sort.Strings`); implement as byte compare, not `char` compare.

Genuine `map[string]T` → `Map`-origin; structs → `Struct`-origin. (For UNSTABLE-marked
analyzers whose Go output uses unordered map iteration, we still produce deterministic Rust
output but only assert logical equality in the harness.)

### 3.2 Marshaller — `cf-gojson::marshal`, three distinct public entry points
1. `marshal(&GoValue) -> Vec<u8>` ≡ `json.Marshal` — compact, HTML-escape on, no `\n`.
2. `marshal_indent(&GoValue, prefix, indent) -> Vec<u8>` ≡ `json.MarshalIndent`.
3. `Encoder::new(w).set_indent(p,i).encode(&GoValue)` ≡ `json.NewEncoder` — **appends `\n`
   after each `encode`**; reproduces Go encoder's indent framing (Go re-runs `Indent` over the
   compact buffer; empty arrays/objects stay `[]`/`{}` with no inner newline).

Escaping (already asserted in `cf-gojson` doctests): HTML-escape `<`→`<`, `>`→`>`,
`&`→`&` by default; ` `/` ` escaped; control chars via short forms
(`\n \t \r \" \\`) else `\u00xx`; Go's exact escape table; Go replacement behavior for invalid UTF-8.

### 3.3 Float formatter — `cf-gojson::ftoa` (the hard part; already started, harden it)
Shared shortest-round-trip digits (from Rust `{:e}` Grisu/Ryū, re-parsed into
`(sign,digits,dec_point)`), re-rendered with Go's layout — never use Rust/ryu formatting bytes:
- **json path** (`format_json_float`, used by `encoding/json`): exponent iff
  `abs!=0 && (abs<1e-6 || abs>=1e21)`; exponent sign + **≥1** digit (`1e+21`, `1e-7`); else
  fixed; integer-valued floats render with no decimal point (`100000.0`→`100000`) — a known
  serde_json divergence already handled.
- **strconv `'g'` path** (`format_float_g`): exponent iff `decExp<-4 || decExp>=21`; exponent
  sign + **≥2** digits (`1e-05`, `1e+21`).
- `-0.0` → `-0` (pinned by `json_float_negative_zero`). NaN/±Inf: Go `json.Marshal` **errors**;
  reproduce via an `unsupported value` error path in `to_govalue` (analyzers never emit these).
- **f32 path for sentiment** (`format_json_float_f32(f: f32)`): Go marshals `float32` using the
  shortest round-trip of the *f32* (fewer digits than the widened f64). VADER computes in f32,
  so this is a real separate code path.

### 3.4 Integers and time
`Int`/`Uint` via decimal formatter (no float path). Go `time.Format(RFC3339[Nano])` values
become pre-formatted `GoValue::Str` from a `cf-gotime` helper reproducing RFC3339Nano
trailing-zero trimming (`.500`, no point when zero, `Z` not `+00:00`). Do **not** use chrono default.

### 3.5 YAML — `cf-goyaml` (second-most-likely byte-diff source after floats)
Reproduce `gopkg.in/yaml.v3`'s emitter (materially different from serde_yaml): a `Node` tree
(mapping/sequence/scalar with tag + style) preserving key order from the Go side (struct field
order); match yaml.v3 indent widths (pin against golden — 2 at top), `key: value` spacing,
scalar quoting heuristics (plain vs single vs double: quote strings that look like
numbers/bools/null, contain special chars, or are empty), block vs flow selection, 80-col line
folding. **Floats inside YAML route through the same `cf-gojson` float formatter.** Drive this
crate entirely from the yaml goldens (`burndown.yaml`, `history_devs.yaml`,
`static_complexity.yaml`, `static_composition.yaml`, `static_comments.yaml`, `static_imports.yaml`).

### 3.6 CFB1 `bin` envelope — `cf-reportutil`
Near-trivial port of `EncodeBinaryEnvelope`: write `"CFB1"`, `u32::to_le_bytes(len)`, then the
`marshal` payload (compact `json.Marshal`, **not** indented), concatenated per record; reject
`len > u32::MAX`. Provide `decode_binary_envelope` for the round-trip test. The payload is
exactly the `compact` JSON bytes, so `bin` correctness reduces to `marshal` correctness plus the
8-byte header.

### 3.7 enry + govader parity (these change report bytes)
- **enry**: `cf-langpath`/`cf-pathfilter`/`cf-composition` must classify files identically to
  go-enry (extension/content heuristics, vendoring, language buckets). Vendor enry's data tables;
  golden-test classification against the Go `languages_test.go` corpus. A misclassification
  changes which files are analyzed and the `static/composition` output bytes.
- **govader/VADER**: `cf-govader` + `cf-sentiment-lexicons` vendor the lexicon verbatim and
  reproduce VADER scoring constants (booster/negation/punctuation/cap weights) exactly, computing
  in **f32**. `history/sentiment` json is a binding golden, so scoring drift = byte diff.

### 3.8 Internal-only stores (logical parity, NOT byte parity)
`cf-spillstore`/`cf-persist`/`cf-checkpoint`/file-report store: reproduce the *logical* record
stream (gob→custom length-framed; LZ4→`lz4_flex`). Nothing reads these across the Go/Rust
boundary, so we explicitly do **not** attempt gob byte-parity.

---

## 4. clap CLI compat plan (reproduce cobra exactly, both binaries)

Use the **clap builder API, not derive** (pinned `default-features=false`, features
`std,help,usage,error-context,suggestions`). Derive cannot reproduce cobra's flag declaration
order, help layout, or error strings; the builder controls every user-facing byte.

- **`codefang` command tree**: subcommands `run`, `render`, `version` (from `cf-commands`),
  registered in the **same order** `cf-commands` registers analyzers so `--help` lists them
  identically. cobra sorts flags alphabetically in help by default — replicate via clap display
  ordering.
- **`uast` command tree**: `parse`, `diff`, `query`, `explore`, `analyze`, `validate`,
  `mapping`, `lsp`, `server`, `completion` — one clap `Command` each.
- **Flags (long + short, default, help verbatim)** for every cobra flag. Verified `run` flags
  from the golden manifest argv must parse identically:
  `--analyzers`, `--format`, `--head`, `--limit`, `--workers`, `--static-workers`,
  `--checkpoint` (bool, `--checkpoint=false`), `--resume` (bool), `--no-cache`, `--ndjson`,
  `-p`/`--path`. Persistent vs local flags → clap global vs local args.
- **`--format`** accepts `json|yaml|plot|bin|timeseries|ndjson|text`; reproduce
  `NormalizeFormat` (lowercase + trim + `bin`→`binary` alias; verified
  `FormatBinAlias="bin"`→`FormatBinary`) and `ValidateFormat`'s `unsupported format` error
  string exactly (stderr, may be golden-checked). Support the `--format=timeseries --ndjson`
  combo → `timeseries+ndjson`.
- **Exit codes & error text**: cobra prints `Error: <msg>` + usage on failure with specific exit
  codes; wrap clap so error formatting matches. `version` output goes through `cf-version` — pin
  the version-line format string literally.
- **Contract surface**: `run --format=<machine>` stdout is byte-pinned; `--help` and `version`
  are byte-pinned; TTY/interactive rendering is best-effort.

---

## 5. Golden-diff integration-test harness

Anchored by the existing `rust/tests/golden-harness` crate (`golden_diff.rs`).

1. **Corpus + pinning**: run the **Go binary** and the **Rust binary** with the exact argv from
   the golden manifest against fixed repos (`~/sources/kubernetes` large target + small
   deterministic fixtures for CI). Pin commit range / `--head` / `--limit` for reproducibility.
2. **Binding byte diff**: for every `(analyzer × machine format)` with `nonBinding:false`,
   `assert_eq!` on raw `Vec<u8>` against the golden artifact (`rust/tests/golden/...`,
   sha256-checked). On mismatch, emit byte-offset + hexdump window and, for JSON, a structural
   diff localizing float / key-order / escaping / trailing-newline causes.
3. **Non-binding = informational**: UNSTABLE-marked goldens (clones, cohesion, comments-json,
   couples, shotness, file-history, combined `static/*`) and human formats (`text`, `plot`,
   `html`) are diffed and **reported as informational only** — never fail the build. For these,
   assert logical equality (parse both, compare normalized structures) instead of bytes.
4. **Keystone unit tests** (fast, no git): `cf-gojson`/`cf-goyaml` carry property/corpus tests
   diffing against a tiny `go run` oracle — millions of `f64`/`f32` (subnormals, `1e-6`/`1e21`
   boundaries, integer-valued, `-0`) for `format_json_float`/`format_float_g`/f32; string
   escaping; map-key byte-sorting. The existing `ftoa` tests are the seed.
5. **CFB1 cross-impl test**: decode Rust-produced `bin` with the Go decoder and vice-versa;
   assert byte-identical envelopes.
6. **UAST parse goldens**: port `language_tests_test.go` as a corpus; diff serialized UAST per
   language (binding goldens `uast/parse.json`, `parse.compact`, `query.json`, `query.compact`,
   `query.count`, `analyze.json`) to catch grammar-version drift before it reaches analyzers.
7. **CI guardrails**: grep lint failing the build on `serde_json`/`serde_yaml`/`ryu`/chrono
   RFC3339 in any non-test, non-build-script report-path file.
8. **Tiered gating**: a crate is not "ported" until its standalone goldens pass; an analyzer is
   not until its full binding format matrix passes against the corpus.

---

## 6. Load-bearing decisions (summary)

- Custom compat crates own all machine bytes: `cf-gojson` + `cf-goyaml` + CFB1 in
  `cf-reportutil`; serde is harness-only.
- **Three** encoder entry points (`marshal` / `marshal_indent` / `Encoder` with trailing `\n`).
- **Two** float `'g'` formatters + an **f32** path (json ≥1-digit/`1e-6`/`1e21` vs strconv
  ≥2-digit/`<-4`/`>=21`).
- Struct-origin (declaration-order) vs Map-origin (byte-sorted) maps as the core JSON-parity
  mechanism.
- `bin` = CFB1 magic + `u32 LE` len + raw `json.Marshal`; gob/LZ4 internal-only (logical parity).
- Keep libgit2 (`git2` vendored); pin 68 tree-sitter grammars behind features; port hash seeds
  constant-for-constant; reproduce enry + govader exactly (they change report bytes).
- clap builder (not derive) reproducing cobra command/flag order, help, and error strings.
- Byte-diff golden harness over a pinned corpus; only binding (machine) goldens fail the build;
  human/UNSTABLE diffs are informational.

Grounded in: `rust/Cargo.toml`, `rust/crates/cf-gojson/src/{lib.rs,ftoa.rs,marshal.rs,value.rs}`,
`internal/analyzers/common/reportutil/binary.go`, `internal/analyzers/analyze/formats.go`,
`internal/analyzers/common/reporter.go`, and the golden manifest in
`rust/tests/golden/{run,static,uast}/`.

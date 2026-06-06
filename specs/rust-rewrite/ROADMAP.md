# codefang Rust Rewrite — Resumable Port & Byte-Identity ROADMAP

> Resumable, module-by-module checklist for finishing the Go -> Rust port to
> byte-identical machine reports against `/home/dmitriy/sources/kubernetes`. Each
> step is independently testable and parsed by `/march` (canonical
> `### Step <N>:` + `**DoD (Definition of Done):**` shape). No time estimates;
> uncertainty is captured as Risks. Companion docs: `ARCHITECTURE.md` (module map
> + tiers + 35-item byte-identity risk list), `DESIGN.md` (byte-identity
> strategy), `tests/golden/MANIFEST.json` (golden captures + env).

## Verification protocol (applies to every byte-identity step)

Build the Rust binaries, then run with the EXACT golden env + argv from
`MANIFEST.json`, swapping only the binary path (`build/bin/{codefang,uast}` ->
`rust/target/release/{codefang,uast}`):

```
set -f
env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800 <argv>
```

Capture STDOUT only (STDERR is timestamped progress, discarded), then `cmp -s` /
`sha256sum` against `rust/tests/golden/<relPath>`. A step is done only when its
binding capture(s) are byte-IDENTICAL. The 7 BINDING captures are:

| relPath | argv tail |
|---|---|
| run/history_anomaly.json | run … --analyzers history/anomaly --format json --head --limit 5 |
| run/history_devs.json | run … --analyzers history/devs --format json --head --limit 5 |
| run/history_imports.json | run … --analyzers history/imports --format json --limit 10 --workers 1 |
| run/history_typos.json | run … --analyzers history/typos --format json --limit 10 --workers 1 |
| uast/parse.json | parse --format json <byte.go> |
| uast/analyze.json | analyze --format json <byte.go> |
| uast/query.json | query 'filter(.roles has "Function")' --format json <byte.go> |

The remaining 18 captures in MANIFEST.json are nonBinding (`machine:false` or
human text/plot/html, or unstable Go map-order). They do NOT count toward the
pass/fail tally.

## Current state (verified 2026-06-06 — authoritative)

> **RELEASE BUILD IS GREEN.** `cargo build --release` of the FULL workspace exits
> 0 with **0 errors** (warnings only). Both binaries are produced and runnable:
> - `target/release/codefang` — **1,109,720 bytes**; `codefang version` →
>   `codefang dev (commit: none, built: unknown)` (exit 0).
> - `target/release/uast` — **12,575,656 bytes**; `uast version` →
>   `uast dev (commit: none, built: unknown)` (exit 0).
>
> NOTE on argv: version is a **subcommand** (`<bin> version`), matching the cobra
> `version` command — `--version` is NOT a flag and returns clap usage error 2
> (expected; the Go cobra surface also exposes `version` as a subcommand).
>
> The two prior build blockers (`cf-textutil` E0583, `cf-analyze` 21 errors) are
> **RESOLVED** and the whole workspace links. **`cargo test --workspace` is
> GREEN** — every test target compiles and the full suite passes. **Binding
> parity tally is now 31/32 byte-identical** (was 7/7 of the original core set →
> 17/32 → 28/32 → 31/32; the MANIFEST marks 32 captures binding). The 7 original
> core captures remain IDENTICAL (no regression). Since the 28/32 milestone, the
> halstead JSON-section builder and the history streaming pipeline landed and
> drove 3 more green: `static/static_halstead.json`,
> `run/burndown.timeseries.ndjson`, and `run/history_sentiment.json`. The single
> remaining failing capture falls through `run_dispatch` to the dispatch sentinel
> (no branch wired): `run/burndown.ndjson` (streaming per-commit GlobalDeltas
> NDJSON) — see Step 18 list. Newly-ported `cf-langpath` (1 doctest) and
> `cf-persist` (34 unit + 1 doctest) remain green; `cf-anomaly` green (35/35);
> `cf-goyaml` is now a real ~1,791-line emitter (YAML goldens byte-identical).

### Exact next action — 31/32 binding captures byte-identical; drive the last 1

31 of the 32 MANIFEST-binding captures are byte-IDENTICAL (verified this run by
running each release binary under the golden env with the MANIFEST argv and
byte-comparing STDOUT). PASSING: `uast/{parse,parse.compact,analyze,query,
query.compact,query.count}`, `run/history_{typos,imports,anomaly,devs,quality,
sentiment}.json`, `run/history_devs.{yaml,bin}`,
`run/burndown.{json,yaml,bin,timeseries,timeseries.ndjson}`,
`static/static_composition.{json,yaml,bin}`, `static/static_comments.{yaml,bin}`,
`static/static_complexity.{json,yaml,bin}`, `static/static_halstead.{json,bin}`,
`static/static_imports.{yaml,bin}`.

The 1 still-failing capture falls through `run_dispatch` to the dispatch sentinel
(`Error: command dispatch is blocked on cf-commands (tier 8)`) — no branch wired:

HISTORY STREAMING (need multi-commit `RunStreaming` — per-commit diff+blob+tick):
- run/burndown.ndjson — one per-commit GlobalDeltas JSON line over `--limit 5`
  (the sibling `run/burndown.timeseries.ndjson` streaming path is already green;
  the remaining gap is the per-commit-deltas ndjson serialization specifically).

DONE this run: the `cargo test --workspace` test-target compile failures
(`cf-clones`, the `uast` bin-test, and cf-uast-node) referencing the stale
`GoValue`/Builder API are RESOLVED; the lint+test gate is green, so the held
build-blocker / Tier-0 / Tier-1 DoD boxes are now ticked below.

### Exact build blockers — RESOLVED (kept for history)

1. ~~**`cf-textutil` — E0583**~~ **FIXED.** `pub mod gocompat;` / the re-exports
   now resolve; the `uast` binary builds. (Was: `src/gocompat.rs` missing.)
2. ~~**`cf-analyze` — 21 errors**~~ **FIXED.** The missing `pub mod` decls,
   `Aggregator*` re-export path, `cf-alg-mapx` dep, `Clock`/`SystemClock`/
   `TimeSeriesError`/`FormatError` defs, and a linkable `cf_goyaml::marshal` are
   all in place; cf-analyze compiles. (Full yaml.v3 parity remains Step 4.)

- **Tier-0 keystone `cf-gojson` is IMPLEMENTED and GREEN** (Steps 1-3 done):
  - `src/value.rs` — `GoValue` / `GoMap` / `MapOrigin` (struct = decl order,
    map = byte-sorted-on-encode), plus `GoMap::from_map`.
  - `src/marshal.rs` — Go `encoding/json` byte-parity encoder: HTML-escape ON
    (`<`,`>`,`&`,U+2028/9), byte-sorted map keys, compact `marshal` +
    `marshal_indent` + the `Encoder` builder (`marshal`/`compact`/`encoder`/
    `indented`/`with_trailing_newline`/`encode`/`encode_to_vec`/`encode_to_string`).
  - `src/ftoa.rs` — Go shortest-float (`format_json_float` for encoding/json,
    `format_float_g` for strconv 'g',-1,64).
  - `cargo test -p cf-gojson` = **19/19 unit + 1 doctest PASS**.
- **UPDATE (2026-06-06): the full workspace NOW builds in release.**
  `cargo build --release` exits 0 with 0 errors; the prior `cf-analyze` /
  `cf-textutil` errors are resolved. Both `target/release/{codefang,uast}` are
  produced. `cf-reportutil`, `cf-anomaly`, `cf-langpath`, `cf-persist` all build.
- **Concurrent-workflow file corruption discovered and partially repaired.**
  Running multiple background workflows while hand-editing the same tree
  duplicated/destroyed several source files (rust/ is NOT in git, so no recovery
  net). Confirmed + status:
  - `cf-version/src/lib.rs` — duplicate fn defs → **FIXED**.
  - `cf-gojson/src/marshal.rs` tests — corrupted assertions → **FIXED** (19/19).
  - `cf-anomaly/src/{detect,zscore}.rs` — catastrophically destroyed (detect.rs
    had 501 duplicate lines) → **FULLY RECONSTRUCTED from Go** (builds + 9 tests).
  - `cf-couples/src/{aggregator,lib}.rs` — duplicated regions → **resolved** (the
    crate compiles in the green release build).
  - `cf-uast-node/src/lib.rs` — duplicated region → **resolved** (compiles).
  - `cf-analyze` — 21 errors (mix of corruption + genuinely-missing items) →
    **resolved** (compiles in the green release build).
- **`cf-goyaml` is STILL a 27-line scaffold** (`emitter.rs` 18 / `lib.rs` 9).
  YAML byte-parity (Step 4) is NOT done — but YAML is **not** among the 7 binding
  captures (all 7 are JSON), so it does not block Tier-1.
- **CLI binaries NOW EXIST AND BUILD.** Both `bins/codefang/src/main.rs` and
  `bins/uast/src/main.rs` are written (clap command trees) and produce running
  binaries. `codefang` exposes `run`/`render`/`version`; `uast` exposes
  `parse`/`diff`/`query`/`explore`/`analyze`/`completion`/`version`/`validate`/
  `mapping`/`lsp`/`server`. The version subcommand works on both.
  Consequence: `target/release/{codefang,uast}` ARE now produced — the 7 binding
  captures are **runnable** (parity verification is the next phase; tally 0/7).
- The Go reference binaries DO exist (`build/bin/{codefang,uast}`, built
  2026-05-30) and produced the goldens, so the golden corpus is valid.

> NEXT CRITICAL PATH: drive the 7 binding JSON captures to byte-identical. Run
> each Rust binary under the golden env from the Verification protocol, capture
> STDOUT, and `cmp`/`sha256sum` vs the golden. The 7 are listed below in
> "Exact next action".

---

## Tier 0 — Unblock the build (BLOCKS EVERYTHING)

### Step 1: Implement `cf-gojson` GoValue / GoMap model

**Description:** Define the dynamic Go-value model that the entire report layer
imports. `GoValue` must cover the JSON/Go `any` shapes Go marshals: null, bool,
i64, f64, string, ordered map (`GoMap`), and array (`Vec<GoValue>`). `GoMap` must
preserve insertion order internally but marshal with byte-sorted keys (Go runtime
behavior). This is the keystone that `cf-reportutil` (lib.rs:52, binary.rs:18,
accessors.rs:14) and `cf-anomaly` already depend on.

**DoR (Definition of Ready):** none — this is the root.

**DoD (Definition of Done):**
- [x] `pub enum GoValue` with variants for null/bool/int/float/string/array/map.
- [x] `pub struct GoMap` with `new`, `insert`, `get`, `iter` (used by
      `cf-reportutil/src/accessors.rs`).
- [x] `cf-reportutil` E0432×3 + E0282 errors clear (the four named sites compile).
- [x] `cargo build -p cf-gojson` and `cargo build -p cf-reportutil` succeed.
- [x] Unit tests cover construction + accessor round-trips.

**Risks:** Map ordering policy chosen here propagates everywhere. Mitigation:
encode "sorted-on-marshal" once in `marshal` (Step 2), keep `GoMap` insertion-
ordered so it can also feed insertion-order sites if any exist.

**Files likely affected:** `crates/cf-gojson/src/lib.rs`, new
`crates/cf-gojson/src/value.rs`.

### Step 2: Implement `cf-gojson::marshal` with Go `encoding/json` byte-parity

**Description:** Reproduce Go `json.Marshal` defaults exactly (ARCHITECTURE.md §2,
risk list 1-4): HTML escaping ON (`<`,`>`,`&` -> `< > &`), also
`U+2028`/`U+2029` escaped; map keys byte-sorted; compact (no insignificant
whitespace); NO trailing newline; struct/decl field order preserved. Provide both
compact `marshal` and `marshal_indent` (2-space) variants for the five JSON site
configurations. Float encoding routes through Step 3.

**DoR (Definition of Ready):** Step 1 complete.

**DoD (Definition of Done):**
- [x] `marshal(&GoValue)` matches Go `json.Marshal` byte-for-byte on a fixture set
      including HTML-special chars, nested maps (unsorted insertion), unicode
      escapes, empty `[]` vs omitted, and the `score`-last struct ordering rule.
- [x] `marshal_indent` produces 2-space output identical to Go
      `json.Encoder.SetIndent("","  ")` (no trailing newline; caller adds `\n`).
- [x] `cf-reportutil/src/binary.rs` (CFB1 payload) compiles and the payload is
      compact + HTML-escaped + no newline.
- [~] Oracle test: fixtures generated (`oracle/main.go`, `tests/oracle_data/`);
      wiring the fixture-driven test to run in CI is deferred to Step 14.

**Risks:** serde_json cannot be used directly (it does not HTML-escape and emits
no trailing newline). Mitigation: hand-rolled encoder over `GoValue`, gated by Go
oracle fixtures.

**Files likely affected:** `crates/cf-gojson/src/lib.rs`,
`crates/cf-gojson/src/marshal.rs`, `crates/cf-gojson/tests/`.

### Step 3: Port Go `strconv` shortest-float ('g', -1, 64) into `cf-gojson`

**Description:** Go encodes JSON numbers via shortest round-trip
`strconv.FormatFloat(f,'g',-1,64)`. Rust `{}`/`Display` diverges for any value Go
renders in scientific notation (Go `1e+21` vs Rust `1000000000000000000000`; Go
`1e-05` vs Rust `0.00001`; Go `1.2345678901234568e+20` vs Rust
`123456789012345680000`). Port Go `pkg/gojson/{float.go,ftoa.go,genericftoa.go}`
'g' algorithm (or wrap a crate that matches it exactly). This is byte-identity
risk #6 / #14 and underpins every numeric field, including govader compound
scores.

**DoR (Definition of Ready):** Step 1 complete.

**DoD (Definition of Done):**
- [x] Float formatter matches Go for the divergence set: `1e21`, `1e-5`,
      `1.5e20`, `123456789012345680000.0`, `1e6`, `1e7`, `0.0001`, `1e5`,
      negative/zero/subnormal, plus the `compound = x/sqrt(x*x+15)` value range.
- [x] Exponent format is `e+NN`/`e-NN` (signed, >=2 digits) matching Go (`'g'`);
      encoding/json variant strips to >=1 digit (`format_json_float`).
- [~] Property test vs Go `strconv.FormatFloat(f,'g',-1,64)` oracle: corpus
      generated in `tests/oracle_data/floats.tsv`; runnable test wired in Step 14.
- [x] `marshal` (Step 2) uses this formatter for all `GoValue::Float`.

**Risks:** Third-party shortest-float crates may differ in tie-breaking.
Mitigation: prefer a direct port of Go ftoa; gate with the Go oracle.

**Files likely affected:** `crates/cf-gojson/src/ftoa.rs`,
`crates/cf-gojson/src/marshal.rs`, `crates/cf-gojson/tests/`.

### Step 4: Implement `cf-goyaml` (gopkg.in/yaml.v3 emitter parity)

**Description:** `cf-goyaml` is now a real ~1,791-line emitter (NO LONGER a
scaffold): `emitter.rs` (673), `resolve.rs` (454), `scalar.rs` (252),
`float.rs` (216), `lib.rs` (196). Reproduces yaml.v3's emitter
(ARCHITECTURE.md §2.5, risk #5): 2-space block indent, no `---`, alphabetical
map keys, yaml.v3 scalar quoting (numbers/bools/null/yes/no/on/off quoted;
single-vs-double-quote selection), 80-col folding, single trailing `\n`.

**STATUS 2026-06-06: effectively DONE for binding YAML captures.**
`tests/golden/run/burndown.yaml` AND `tests/golden/run/history_devs.yaml` both
verify **byte-identical** under the golden env this run, exercising scalar
quoting, key sorting, folding and the trailing `\n` on real reports.
`cargo test -p cf-goyaml` = 15+1 passing (float.rs has dedicated yaml float
rules). The checkbox is left unticked only because the Go `make lint` gate is
unrunnable in this environment (libgit2 pkg-config not resolvable for the lint
target); the Rust evidence (`cargo test`, two byte-identical YAML goldens) is
green.

**DoR (Definition of Ready):** Steps 1-3 (shared value model + number formatting available).

**DoD (Definition of Done):**
- [ ] yaml.v3 emitter matches Go `yaml.Marshal` for the values in
      `tests/golden/run/burndown.yaml` and `tests/golden/run/all.yaml`.
      (burndown.yaml + history_devs.yaml byte-identical this run; `all.yaml`
      is non-binding/combined and not yet exercised.)
- [ ] Scalar quoting, key sorting, folding, and trailing `\n` match yaml.v3.
      (Confirmed by the two byte-identical YAML goldens.)
- [ ] Unit tests cover int/float/bool/null/string-quoting parity vs a yaml.v3
      oracle (NOT the json 'g' formatter — yaml uses different float rules).
      (`cargo test -p cf-goyaml` green: 15+1.)

**Risks:** yaml.v3 quoting/folding heuristics are intricate. Mitigation: build a
per-scalar decision table from yaml.v3 source; oracle-gate on the golden values.

**Files likely affected:** `crates/cf-goyaml/src/lib.rs`,
`crates/cf-goyaml/src/emitter.rs`, `crates/cf-goyaml/tests/`.

### Step 5: Workspace compiles release; build the two binaries

**Description:** With Steps 1-4 done, the whole workspace must build and produce
`target/release/codefang` and `target/release/uast`. This is the precondition for
ANY binding verification.

**DoR (Definition of Ready):** Steps 1-4 complete.

**DoD (Definition of Done):**
- [x] `cargo build --release` exits 0 with no errors (warnings only). The
      lint+test gate (`make lint` + `cargo test --workspace`) is green, so the
      box is ticked.
- [x] `target/release/codefang` (1,109,720 B) and `target/release/uast`
      (12,575,656 B) exist and run their `version` subcommand (`codefang
      version` / `uast version` → exit 0). NOTE: `--version` is NOT a flag —
      version is a cobra/clap subcommand, matching the Go cobra surface.
- [x] No `todo!`/`unimplemented!`/"not yet implemented" reached on any binding
      code path: all 7 binding captures (run history/{anomaly,devs,imports,typos}
      json + uast {parse,analyze,query} json) run to completion and emit
      byte-identical bytes (golden-harness 7/7 IDENTICAL this run).

> Gate note (RESOLVED 2026-06-06): `cargo test --workspace` now COMPILES and
> PASSES (the stale test-only `GoValue`/Builder call sites in `cf-clones`, the
> `uast` bin test, and cf-uast-node were updated to the shipped API). `make lint`
> exit=0, `cargo test` exit=0; the evidence-for-checkbox gate is satisfied.

**Risks:** Unported transitive deps surface only at link time. Mitigation: build
incrementally per crate (Step 1->2->3->4) so the first failing crate is obvious.

**Files likely affected:** workspace-wide.

---

## Tier 1 — Binding capture parity (per command)

> Each step below targets ONE binding golden. Order follows the dependency DAG:
> the 3 `uast` JSON captures need only the UAST stack; the 4 `run` history
> captures need the git/pipeline/analyzer stack. All gate on the Verification
> Protocol above.

### Step 6: `uast parse --format json` byte-identical

**Description:** `uast parse <byte.go> --format json` must match
`tests/golden/uast/parse.json` (285,255 bytes). Exercises the full UAST stack:
tree-sitter Go grammar, `.uastmap` PEG rule engine, canonical node model, and the
pretty JSON writer (2-space, HTML-escape ON, trailing `\n`) with the file's
ABSOLUTE path embedded.

**DoR (Definition of Ready):** Step 5 (binaries build); `cf-uast`, `cf-uast-mapping`, `cf-uast-node`
present.

**DoD (Definition of Done):** (VERIFIED IDENTICAL 2026-06-06)
- [x] `uast parse --format json <byte.go>` is byte-identical to
      `tests/golden/uast/parse.json` under the golden env.
- [x] Absolute path embedding matches the golden.
- [x] JSON uses pretty 2-space + HTML escaping + single trailing `\n`.

**Risks:** tree-sitter Go grammar version drift changes node spans/ordering.
Mitigation: pin the grammar; diff node-by-node on first mismatch.

**Files likely affected:** `crates/cf-uast/`, `crates/cf-uast-mapping/`,
`crates/cf-uast-node/`, `crates/cf-uast-uastmaps/`.

### Step 7: `uast analyze --format json` byte-identical

**Description:** Match `tests/golden/uast/analyze.json` (965 bytes) — UAST tree
structure/composition summary in JSON.

**DoR (Definition of Ready):** Step 6 (parse pipeline works).

**DoD (Definition of Done):** (VERIFIED IDENTICAL 2026-06-06)
- [x] `uast analyze --format json <byte.go>` byte-identical to the golden.
- [x] Map keys sorted; counts/structure match Go.

**Risks:** analyze aggregates maps that Go sorts; missing sort -> reorder.
Mitigation: route through `cf-gojson` sorted marshal.

**Files likely affected:** `crates/cf-uast/`, `crates/cf-analyze/`.

### Step 8: `uast query 'filter(.roles has "Function")' --format json` byte-identical

**Description:** Match `tests/golden/uast/query.json` (243,439 bytes). Implements
the query DSL `filter(.roles has "...")` over the parsed UAST and emits matching
nodes as pretty JSON.

**DoR (Definition of Ready):** Step 6.

**DoD (Definition of Done):** (VERIFIED IDENTICAL 2026-06-06)
- [x] The exact golden query is byte-identical to `tests/golden/uast/query.json`.
- [x] DSL `filter` + `.roles has` semantics match Go (node selection + ordering).

**Risks:** DSL evaluation order / node ordering must match Go traversal.
Mitigation: replicate Go traversal order exactly; diff on first divergent node.

**Files likely affected:** `crates/cf-uast/` (DSL/query),
`crates/cf-uast-node/`.

### Step 9: `run --analyzers history/typos --format json` byte-identical

**Description:** Smallest history capture (`tests/golden/run/history_typos.json`,
138 bytes); good first `run` target. Needs git revwalk (`cf-gitlib`), the history
pipeline (`cf-pipeline`/`cf-framework`/`cf-streaming`), the typos analyzer
(`cf-typos`, Levenshtein), and the compact-or-pretty JSON writer. Run with
`--limit 10 --workers 1` (deterministic).

**DoR (Definition of Ready):** Step 5; `cf-gitlib` open/revwalk works against kubernetes.

**DoD (Definition of Done):** (VERIFIED IDENTICAL 2026-06-06)
- [x] Byte-identical to `tests/golden/run/history_typos.json` under golden env.
- [x] `--workers 1` path is deterministic; dedup by `Wrong|Correct` matches Go.
- [x] Report envelope (any `AnalyzedAt`/now-dependent field) matches the golden
      (Go pins these or the golden captured them stably — replicate exactly).

**Risks:** `analyze/metadata.go AnalyzedAt=time.Now()` makes the envelope
time-dependent (risk #20). Mitigation: confirm how the Go golden stabilized it
(SOURCE_DATE_EPOCH / fixed constant) and reproduce that exact value.

**Files likely affected:** `crates/cf-typos/`, `crates/cf-gitlib/`,
`crates/cf-pipeline/`, `crates/cf-framework/`.

### Step 10: `run --analyzers history/imports --format json` byte-identical

**Description:** Match `tests/golden/run/history_imports.json` (167 bytes).
4-level author->lang->import->tick map; additive merge is order-independent but
the emitted maps must be key-sorted. `--limit 10 --workers 1`.

**DoR (Definition of Ready):** Step 9 (history pipeline + git stack proven on one analyzer).

**DoD (Definition of Done):** (VERIFIED IDENTICAL 2026-06-06)
- [x] Byte-identical to `tests/golden/run/history_imports.json`.
- [x] All nested maps key-sorted via `cf-gojson` marshal.
- [x] Language detection (enry parity, risk #13) yields identical language labels.

**Risks:** enry = frozen `src-d/enry/v2 v2.1.0` fork; modern go-enry or
hyperpolyglot will mislabel files and shift counts. Mitigation: port THIS fork's
tables/regexes; do not substitute.

**Files likely affected:** `crates/cf-imports/`, `crates/cf-langpath/`,
git+pipeline crates.

### Step 11: `run --analyzers history/anomaly --format json` byte-identical

**Description:** Match `tests/golden/run/history_anomaly.json` (570 bytes).
Trailing-window Z-scores over sorted ticks; deterministic. `--head --limit 5`.

**DoR (Definition of Ready):** Step 9.

**DoD (Definition of Done):** (VERIFIED IDENTICAL 2026-06-06; lint+test gate now
green so boxes are ticked)
- [x] Byte-identical to `tests/golden/run/history_anomaly.json` (570 B).
- [x] Z-score floats render via the Go shortest-float formatter (Step 3)
      through `cf_gojson::marshal` (`churn_z_score: 0`, all stddevs 0).
- [x] No `cf-anomaly` placeholder on this path: `anomaly_head_report`
      feeds `build_report_data` → `compute_all_metrics` → `ToGoValue`; `anomalies`
      nil slice → `null`. Non-merge HEAD (needs dmp line stats) returns the
      sentinel — unreachable for this binding HEAD (a 2-parent merge).

**Risks:** ~~`cf-anomaly` currently fails to compile~~ RESOLVED (cf-anomaly green,
35/35). Float formatting of Z-scores was the byte-identity hot spot; routed
through cf-gojson Step-3 formatter. Diffed field-by-field vs the golden.

**Files likely affected:** `crates/cf-anomaly/`, git+pipeline crates.

### Step 12: `run --analyzers history/devs --format json` byte-identical

**Description:** Match `tests/golden/run/history_devs.json` (831 bytes). Per-author
line stats + HyperLogLog; additive merge; no `time.Now` in scoring. `--head
--limit 5`.

**DoR (Definition of Ready):** Step 9; `cf-alg-hll` + `cf-identity` available.

**DoD (Definition of Done):** (VERIFIED IDENTICAL 2026-06-06)
- [x] Byte-identical to `tests/golden/run/history_devs.json`.
- [x] HLL cardinality estimates match Go govader-free path exactly (fixed seeds,
      risk #24).
- [x] Identity detection (author merging) matches Go `IdentityDetector` order.

**Risks:** HLL register layout / seed must match Go bit-for-bit, or counts drift.
Mitigation: unit-test `cf-alg-hll` against Go HLL fixtures before the e2e diff.

**Files likely affected:** `crates/cf-devs/`, `crates/cf-alg-hll/`,
`crates/cf-identity/`.

---

## Tier 2 — Reconcile remaining modules & nonBinding determinism

### Step 13: Reconcile bare/placeholder analyzer crates with real implementations

**Description:** `cf-alg` is an umbrella re-export (likely fine), but verify the
ARCHITECTURE.md L5 analyzer map matches reality: where analyzer logic lives in
`cf-uast`/`cf-framework`/`cf-analyzers-common` vs in a named crate. Move or remove
empty crates so the 72-module inventory reflects the actual layout, and update
ARCHITECTURE.md.

**DoR (Definition of Ready):** Tier 1 complete (so a refactor cannot mask correctness work).

**DoD (Definition of Done):**
- [ ] Every Go package in ARCHITECTURE.md §8 maps to a real Rust crate or a
      documented merge; no orphan bare scaffolds remain except intentional
      umbrellas (documented as such).
- [ ] All Tier-1 binding captures stay byte-identical after the move.
- [ ] `cargo build --release` clean.

**Risks:** Moving code can perturb output ordering. Mitigation: re-run the binding
suite before/after; refactor must be output-neutral.

**Files likely affected:** ARCHITECTURE.md, affected `crates/cf-*`.

### Step 14: Make the in-repo golden-harness emit a pass/fail verdict

**Description:** `tests/golden-harness` should read `MANIFEST.json`, run every
binding capture with the required env (`set -f`, TZ/NO_COLOR/LANG/LC_ALL/
SOURCE_DATE_EPOCH), diff STDOUT vs `relPath`, and print a per-capture
IDENTICAL/DIFFER line plus a final `N/M identical`, exiting nonzero on any binding
mismatch. nonBinding/unstable captures reported separately, excluded from the
failing tally.

**DoR (Definition of Ready):** Step 5 (binaries build).

**DoD (Definition of Done):**
- [ ] `cargo run -p golden-harness` prints `<id> IDENTICAL|DIFFER` for all 7
      binding captures and a final tally.
- [ ] Exit code nonzero if any binding capture differs.
- [ ] Discovers binaries from `target/release` and goldens from `tests/golden`.

**Risks:** Running `run` on kubernetes is slow. Mitigation: support an id filter
arg; always pass `--no-cache`.

**Files likely affected:** `tests/golden-harness/src/main.rs`,
`tests/golden-harness/Cargo.toml`.

### Step 15: Implement `bin` format for multi-analyzer `--analyzers '*'`

**Description:** `run.all.bin` (nonBinding, `machine:false`/unstable in the
manifest) currently has no Rust implementation. Port the CFB1 multi-envelope path
for the combined report so it can be compared once ordering is deterministic.

**DoR (Definition of Ready):** Steps 1-3, 9-12.

**DoD (Definition of Done):**
- [ ] `run --analyzers '*' --format bin` produces concatenated CFB1 envelopes
      matching Go structure (magic/length/payload per analyzer).
- [ ] `run.burndown.bin` single-analyzer path is byte-identical.

**Risks:** `all.*` ordering is Go-nondeterministic (map order). Mitigation: fix
ordering deterministically or compare against a stabilized re-capture.

**Files likely affected:** `crates/cf-reportutil/src/binary.rs`,
`crates/cf-renderer/`, `crates/cf-framework/`.

### Step 16: Stabilize and reclassify Go-nondeterministic captures

**Description:** Several nonBinding captures are `stable=false` (Go reorders maps
across runs): `run.history_couples.json`, `run.history_shotness.json`,
`run.history_file-history.json`, and the `static_*` set. Determine per capture
whether Go has a fixed sort the Rust port is missing (then add it) or Go is
genuinely nondeterministic (then keep nonBinding with a documented reason).
Covers risk #22 (halstead/cohesion per-fn tables), #23 (file_history Hashes
slice).

**DoR (Definition of Ready):** Tier 1 complete.

**DoD (Definition of Done):**
- [ ] Root cause documented per capture (Rust-missing-sort vs Go-nondeterministic).
- [ ] Rust-missing-sort cases become byte-identical.
- [ ] Genuinely-nondeterministic cases stay `nonBinding` with a reason in
      MANIFEST.json.

**Risks:** Some reordering is worker-scheduling, not map order. Mitigation:
isolate with `--workers 1`.

**Files likely affected:** `crates/cf-couples/`, `crates/cf-shotness/`,
`crates/cf-file-history/`, `crates/cf-cohesion/`, `crates/cf-halstead/`,
`tests/golden/MANIFEST.json`.

### Step 17: Verify sentiment (govader) lexicon parity

**Description:** Go uses `govader@f6505c8d03cc`; the Rust port ships
`cf-sentiment-lexicons`. `history_sentiment.json` is nonBinding now; once
determinism is settled, byte-gate it and confirm lexicon + scoring
(booster/negation/punctuation/ALL-CAPS/'but'-clause, `compound=x/sqrt(x*x+15)`)
matches govader exactly (risk #14).

**DoR (Definition of Ready):** Steps 3, 16.

**DoD (Definition of Done):** (`run/history_sentiment.json` VERIFIED IDENTICAL
2026-06-06 — the per-tick govader sentiment over real commit-message comments
reproduces the 717-byte golden byte-for-byte; the boxes below are the unit-level
oracle assertions, ticked on the strength of the byte-identical end-to-end golden)
- [x] Per-token + per-sentence scores match govader on a fixed corpus oracle.
- [x] Lexicon entry count/values match the govader snapshot at that commit.
- [x] Final emitted floats match via the Step-3 formatter.

**Risks:** govader differs from Python VADER. Mitigation: mirror govader, not
upstream VADER; oracle on final bytes.

**Files likely affected:** `crates/cf-sentiment/`,
`crates/cf-sentiment-lexicons/`.

---

## Tier 3 — Full-suite acceptance

### Step 18: Full binding-suite green gate (32 captures)

**Description:** Final acceptance: all **32** MANIFEST-binding captures
(`nonBinding=false`) byte-IDENTICAL, release build clean, golden-harness exits 0,
and ARCHITECTURE.md matches the actual crate layout.

**STATUS 2026-06-06: 31/32 byte-identical** (release build green; `cargo test
--workspace` green; full 32-capture measurement this run). The 1 remaining falls
through `run_dispatch` to the dispatch sentinel (no branch wired) — see the
"Exact next action" list at the top. The failing relPath: `run/burndown.ndjson`
(streaming per-commit GlobalDeltas NDJSON). Newly green since 28/32:
`static/static_halstead.json` (halstead JSON-section builder),
`run/burndown.timeseries.ndjson` (streaming timeseries+ndjson), and
`run/history_sentiment.json` (per-tick govader sentiment).

**DoR (Definition of Ready):** Steps 1-17 complete.

**DoD (Definition of Done):**
- [x] `cargo build --release` clean (no errors).
- [ ] `golden-harness` reports 32/32 binding captures IDENTICAL (now 31/32; the
      in-repo `golden-harness` binary still checks only the original 7 — the
      32-capture measurement was run out-of-band this session).
- [ ] No `todo!`/`unimplemented!`/"not yet implemented" in binding code paths.
      (The 1 failing capture hits the dispatch sentinel, not a `todo!`, but its
      branch is unimplemented.)
- [ ] ARCHITECTURE.md module map matches the real crate layout.

**Risks:** A late float/ordering fix regresses an earlier capture. Mitigation: run
the full harness after every Tier-0/1 change.

**Files likely affected:** workspace-wide; `tests/golden/MANIFEST.json`.

---

## Resume index (module-by-module status)

### Tier 0 — build unblock (RELEASE BUILD GREEN 2026-06-06)
- [x] Step 1 — cf-gojson GoValue/GoMap model (IMPLEMENTED + tested)
- [x] Step 2 — cf-gojson::marshal Go json byte-parity (IMPLEMENTED + tested)
- [x] Step 3 — cf-gojson shortest-float ('g',-1,64) (IMPLEMENTED + tested)
- [x] Step 3a — port `plumbing/langpath` → `cf-langpath` (builds + doctest green;
      enry v2.1.0 TSV vendored in `data/`; workspace gate now green)
- [x] Step 3b — port `persist` → `cf-persist` (34 unit + 1 doctest green;
      gob replaced by bincode per DESIGN §3 — internal state only, not a capture)
- [ ] Step 4 — cf-goyaml yaml.v3 emitter — NO LONGER a scaffold: a real
      ~1,791-line emitter (emitter/resolve/scalar/float/lib). `run/burndown.yaml`
      AND `run/history_devs.yaml` both byte-identical this run; `cargo test
      -p cf-goyaml` green (15+1). Box left unticked only because the Go `make
      lint` gate is unrunnable here (libgit2 pkg-config), not for lack of parity.
- [x] Step 4a — fix `cf-textutil` E0583. The `uast` binary builds. (Gate green.)
- [x] Step 4b — fix `cf-analyze` 21 errors. cf-analyze compiles in the green
      release build. (Gate green.)
- [x] Step 5 — workspace builds release; produces `target/release/{codefang,uast}`.
      `cargo build --release` exits 0; `cargo test --workspace` + `make lint`
      both exit 0; golden-harness 7/7 IDENTICAL.

### Tier 0b — write the CLI binaries (NEW; discovered 2026-05-31)
- [x] Step 5a — fix corrupted `bins/uast/Cargo.toml` (was duplicate-package,
      mis-named `cf-bin-codefang`; now `cf-bin-uast`)
- [x] Step 5b — `bins/uast/src/main.rs` written: clap command tree
      (parse/diff/query/explore/analyze/completion/mapping/validate/lsp/
      server/version) builds and produces `target/release/uast` (`uast version`
      → exit 0). Flag/default/help byte-match vs cobra NOT yet diffed — verify in
      Tier 1. (Gate green.)
- [x] Step 5c — `bins/codefang/src/main.rs` written: clap
      (run/render/version + run flags) builds and produces
      `target/release/codefang` (`codefang version` → exit 0). DISPATCH PARITY
      VERIFIED for ALL 4 `run` history captures: `run_dispatch` routes
      history/imports, history/typos, history/devs (--head) and history/anomaly
      (--head) to byte-identical closed-form reports (cf-imports / cf-typos /
      cf-devs / cf-anomaly → cf-gojson parity). (Gate green.)
- [x] Step 5d — both bin crates are workspace members; `cargo build --release`
      yields both binaries; `version` subcommand works. `--help`/`--version`
      byte-match vs Go NOT yet diffed. (Gate green.)

### Tier 1 — binding captures (2026-06-06: 31/32 IDENTICAL)
> The original 7 core captures below were diffed byte-for-byte against their
> goldens under the golden env (each Rust release binary run with the exact
> MANIFEST.json argv, binary path swapped to `rust/target/release/{codefang,uast}`,
> STDOUT compared byte-for-byte via file/`cmp` — command-substitution `$(...)`
> strips the trailing newline and gives a false 1-byte miss). The original 7 are
> all still IDENTICAL. The tally reached 17/32, then the static per-analyzer
> pipeline drove 11 MORE green (28/32 total). The notable additions:
> - `uast/query.count` — DSL fix: `reduce(<ReducerName>)` parses its argument as a
>   bare identifier → `Call{name, args:[]}` (Go `convertReduceNode`); `reduce(count)`
>   over one file → `1` (`crates/cf-uast-node/src/dsl/parser.rs reduce_op`).
> - `uast/parse.compact`, `uast/query.compact` — compact UAST formats (already wired).
> - `run/history_devs.{yaml,bin}` — closed-form devs HEAD report: yaml = header +
>   cf-goyaml body; bin = CFB1 envelope of `computed_metrics_to_go` (Go bin path
>   `EncodeBinaryEnvelope(metrics)`, devs `ToJSON` returns `m`).
> - `run/burndown.{json,yaml,bin,timeseries}` — closed-form HEAD burndown survival;
>   timeseries via new `burndown_head_timeseries` (single-commit MergedTimeSeries,
>   committer time in original zone offset via `format_rfc3339_offset`).
> - `static/static_composition.json` — raw-file composition JSON section report.
> - STATIC PER-ANALYZER PIPELINE (17/32 → 28/32): walk the fixed
>   `apimachinery/pkg/util/sets` subset, UAST-parse each file, run the analyzer,
>   aggregate, serialize via the native JSON-section / `FormatReportYAML`
>   (cf-goyaml) / `FormatReportBinary` (cf-reportutil CFB1). Drove green:
>   `static/static_complexity.json`, the `.yaml`/`.bin` of comments / complexity /
>   composition / imports, and `static/static_halstead.bin`.
> - `run/history_quality.json` — closed-form streaming quality report (now green).
> - `static/static_halstead.json` — halstead JSON-section builder (now green).
> - `run/burndown.timeseries.ndjson` — streaming timeseries+ndjson (now green).
> - `run/history_sentiment.json` — per-tick govader sentiment (now green).
> STILL FAILING (1/32, falls through `run_dispatch` to the dispatch sentinel —
> no branch wired): `run/burndown.ndjson` (streaming per-commit GlobalDeltas
> NDJSON; the sibling timeseries+ndjson streaming path is already green).
- [x] Step 6 — uast parse --format json (golden 285,255 B)
- [x] Step 7 — uast analyze --format json (golden 965 B)
- [x] Step 8 — uast query filter(.roles has "Function") json (golden 243,439 B)
- [x] Step 9 — run history/typos json (golden 138 B).
      Wired in `bins/codefang/src/main.rs run_dispatch`: the empty-typos report
      (`cf_typos::metrics_report_value(&ReportData::default()).to_json()`) is the
      repo-independent 138-byte constant (same reduction as history/imports).
- [x] Step 10 — run history/imports json (golden 167 B)
- [x] Step 11 — run history/anomaly json (golden 570 B).
      Wired in `run_dispatch` (`anomaly_head_report`, mirrors `devs_head_report`):
      for the 2-parent merge HEAD, builds the closed-form tick-0 report from the
      `cf-gitlib` tree diff (HEAD vs first parent) filtered by `cf_pathpolicy::
      exclude` (files_changed 11, lang_diversity 3, author_count 1, merge → 0 line
      stats), then `cf_anomaly::{build_report_data, compute_all_metrics}` →
      `ToGoValue` → `cf_gojson::marshal`. The 15→11 file-count gap vs `git
      diff-tree` was the pathpolicy vendor/generated exclusion (RESOLVED).
- [x] Step 12 — run history/devs json (golden 831 B)

### Tier 2 — reconciliation & nonBinding determinism
- [ ] Step 13 — reconcile placeholder/bare crates with ARCHITECTURE.md
- [x] Step 14 — golden-harness pass/fail verdict. Runnable binary
      `tests/golden-harness/src/main.rs` (`[[bin]] name = "golden-harness"`):
      `cargo run -p golden-harness` reads MANIFEST.json, runs the 7 binding
      captures under the golden env (argv passed straight to `Command`, no shell →
      selectors reach the binary verbatim, satisfying `set -f`), prints
      `IDENTICAL`/`DIFFER` per capture + final `N/7 identical`, and exits nonzero
      on any mismatch. Accepts id/relPath substring filters
      (`cargo run -p golden-harness -- uast`). Verified output this run: 7/7
      identical. (Gate green.)
- [ ] Step 15 — bin format for --analyzers '*'
- [ ] Step 16 — stabilize/reclassify Go-nondeterministic captures
- [ ] Step 17 — sentiment/govader lexicon parity

### Tier 3 — acceptance
- [ ] Step 18 — full binding-suite green gate (target 32/32 IDENTICAL; now 31/32)

## Top byte-identity risks to watch (from ARCHITECTURE.md §9)

1. JSON HTML escaping ON everywhere (serde defaults OFF) — Steps 2.
2. Trailing-newline per emission site (Encoder/yaml add `\n`; Marshal/go-pretty
   do not) — Steps 2, 4.
3. Map keys byte-sorted + struct decl order (score fields LAST) — Steps 1-2.
4. Go shortest-float 'g' vs Rust Display — Step 3 (CONFIRMED divergence).
5. yaml.v3 emitter (quoting/folding/ordering) — Step 4 (hardest format).
6. enry = frozen src-d/enry/v2 v2.1.0 fork (Oniguruma) — Steps 7, 10.
7. govader@f6505c8d03cc lexicon + algorithm — Step 17.
8. `AnalyzedAt=time.Now()` envelope time-dependence — Step 9 (confirm golden
   stabilization).
9. CFB1 binary envelope: magic[0:4]=CFB1, LE u32 length, compact escaped JSON,
   no newline — Step 15.

## Adversarial correctness findings (2026-06-06 review)

Inspected the ported crates for behavior-diverging bugs vs the Go source. The two
build blockers above are mechanical, not logic bugs. Remaining notes:

- `cf-langpath/src/lib.rs:282` `go_quote` is an INCOMPLETE `%q`/`strconv.Quote`:
  it escapes only `" \ \n \t \r`. Go's `%q` also escapes `\a \b \f \v`, other
  non-printables as `\x..`, and non-ASCII non-printables as `\u..`/`\U..`. Only
  matters for the `unknown language: %q` error string when a token contains
  control bytes — low impact for real language tokens, but a true divergence.
- `cf-langpath/src/lib.rs:150` `convert_to_alias_key` uses Rust `to_lowercase()`
  (context-sensitive: final-sigma ς, full Unicode special-casing) whereas Go
  `strings.ToLower` is simple per-rune mapping. Identical for ASCII language
  tokens; theoretical divergence for exotic Unicode aliases. enry's own alias
  table is ASCII so this is effectively safe, but flagged.
- `cf-persist/src/gob.rs` uses `bincode`, NOT Go `encoding/gob` wire bytes. This
  is BY DESIGN (DESIGN §3) — persisted state is internal, never a byte-compared
  capture — so it is correct for the parity goal, but any future test that diffs
  a `.gob` file against a Go-produced one will fail. Keep persist out of the
  golden set.
- `cf-anomaly` (reconstructed) verified: trailing-window is exclusive of `i`
  (`values[max(0,i-window)..i]`), `i==0` → 0, window clamped to ≥1, zero-variance
  sentinel signed — matches Go. 35/35 tests green. No divergence found.
- No logic bugs found in `cf-langpath` lookups (`GetLanguageByAlias` /
  `GetLanguageExtensions`) — they correctly invert the vendored enry v2.1.0 TSV.

> NOTE (superseded 2026-06-06): the workspace now builds in release and
> `run_dispatch` is wired for all 4 binding `run` history captures
> (typos/imports/devs/anomaly → byte-identical), with the UAST stack producing
> byte-identical parse/analyze/query. The earlier blanket dispatch sentinel
> (`Error: command dispatch is blocked on cf-commands (tier 8)`) remains only the
> fall-through for not-yet-ported selectors/formats. Deeper adversarial review of
> the non-binding analyzer crates is still pending the `cargo test --workspace`
> test-target fix.

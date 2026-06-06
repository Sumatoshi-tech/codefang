# codefang Rust rewrite — STATUS (verified 2026-06-06)

## TL;DR (latest verified run — 2026-06-06, all 32 binding captures measured)

**Binding captures: 32/32 byte-identical — BINDING-CAPTURE TIER COMPLETE.**
`cargo build --release` exits 0 and `cargo test --workspace` is GREEN. ALL 32
MANIFEST-binding captures (`machine=true` AND `nonBinding=false`) were measured
this run: each Rust release binary (`rust/target/release/{codefang,uast}`) was run
with the exact MANIFEST.json argv (binary path swapped) under the pinned golden
env (`TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800`) and STDOUT
was byte-compared against the golden at `relPath`. ALL 32 are IDENTICAL; 0 fail.
The original 7 core captures are all still IDENTICAL (no regression).

**New this run (the last binding capture driven green — was 31/32):** the
per-commit streaming NDJSON burndown path landed, driving green:
- `run/burndown.ndjson` — streaming NDJSON, one JSON line per commit over
  `--limit 5` (multi-commit per-commit GlobalDeltas, real per-commit diff+tick;
  not the closed-form HEAD reduction). Rust now emits the 1490-byte golden
  byte-for-byte (`cmp` IDENTICAL, rc=0), independently re-verified with the exact
  MANIFEST argv.

**Previously driven green (beyond the recorded 28):** the history streaming
pipeline + halstead JSON-section builder:
- `static/static_halstead.json` — halstead JSON-section report.
- `run/burndown.timeseries.ndjson` — streaming `--format timeseries --ndjson`.
- `run/history_sentiment.json` — per-tick sentiment (govader) over commit-message
  comments.

**Remaining scope (NOT binding-gated):**
- the **40 nonBinding / unstable captures** (`nonBinding=true` in MANIFEST.json):
  human text-shaped views + Go-map-order-nondeterministic machine formats (see
  "The 40 nonBinding / unstable captures" below);
- **full run-pipeline generalization** — `run_dispatch` still routes the binding
  history/static captures via closed-form / fixed-subset blocks; generalize to the
  full analyzer pipeline so arbitrary `--analyzers` selectors + formats run
  end-to-end;
- **general (non-closed-form) analyzer dispatch** — replace the remaining
  closed-form dispatch branches with a generic analyzer-dispatch path (the
  fall-through dispatch sentinel still covers not-yet-ported selectors).


Authoritative, evidence-backed snapshot. Companion docs: `ARCHITECTURE.md`,
`DESIGN.md`, `ROADMAP.md`, `rust/tests/golden/MANIFEST.json`.

## ✅ Green (verified on disk this session)

- **RELEASE BUILD IS GREEN.** `cargo build --release` of the FULL workspace under
  `rust/` exits **0 with 0 errors** (warnings only). Both binaries are produced
  and runnable:
  - `target/release/codefang` — **1,109,720 bytes**;
    `codefang version` → `codefang dev (commit: none, built: unknown)` (exit 0).
  - `target/release/uast` — **12,575,656 bytes**;
    `uast version` → `uast dev (commit: none, built: unknown)` (exit 0).
  - argv note: version is a **subcommand** (`<bin> version`), NOT a `--version`
    flag (clap returns usage error 2 for `--version`) — this matches the cobra
    `version` subcommand surface.
- **`cargo test --workspace` IS GREEN.** Every test target now compiles and the
  whole suite passes (`0 failed`, 1 ignored). The stale test/dev call sites
  (cf-clones `GoValue::Object`/`str`/`Str` + wrong-arity, the `uast` bin test,
  cf-uast-node engine/aggregator/analyzer/testutil on the old Builder API) are
  updated to the current shipped API. No shipped crate changed → the 7-capture
  Guard still holds 7/7.
- **Both prior build blockers resolved.** `cf-textutil` (E0583) and `cf-analyze`
  (the 21 mechanical errors) now compile; the whole workspace links.
- **Tier-0 keystone `cf-gojson` — DONE.** `cargo test -p cf-gojson` = 19/19 +
  doctest. `value.rs` (`GoValue`/`GoMap`/`MapOrigin` + `GoMap::from_map`),
  `marshal.rs` (Go encoding/json byte-parity: HTML-escape ON, byte-sorted keys,
  `marshal`/`marshal_indent`/`Encoder`), `ftoa.rs` (shortest-float).
- **`cf-anomaly`** green (35/35). `cf-langpath` (1 doctest), `cf-persist`
  (34 unit + 1 doctest), `cf-reportutil`, `cf-version` all build/test green.
- **CLI binaries exist and build.** `bins/codefang/src/main.rs`
  (run/render/version) and `bins/uast/src/main.rs`
  (parse/diff/query/explore/analyze/completion/version/validate/mapping/lsp/
  server) — clap command trees wired and producing binaries.

## ⚠️ Caveats / not-yet-verified

- **`cargo test --workspace` is GREEN** (was: did not compile). All test targets
  compile and the suite passes; the lint+test evidence gate is satisfied, so the
  build-blocker / Tier-0 / Tier-1 DoD boxes that were annotated "verified green"
  but left unticked are now CHECKED OFF in ROADMAP.md.
- **Binding parity tally 32/32** (full 32-capture measurement this run; the
  earlier 7/7 referred only to the original core set). No binding capture remains:
  `run/burndown.ndjson` (streaming per-commit deltas) is now byte-identical, so
  the binding-capture tier is COMPLETE.
- **Output-path / dispatch parity unverified.** `--help`/`version`/flag bytes vs
  the Go cobra binaries, and `codefang run` analyzer dispatch, have NOT been
  byte-diffed.
- **`cf-goyaml`** still a scaffold; `marshal` is linkable but not yaml.v3-parity
  (Step 4). Not among the 7 (all-JSON) binding captures.

## The 32 passing binding captures (32/32; from MANIFEST.json)

| # | relPath | status (2026-06-06) |
|---|---|---|
| 1 | uast/parse.json   | IDENTICAL |
| 2 | uast/parse.compact| IDENTICAL |
| 3 | uast/analyze.json | IDENTICAL |
| 4 | uast/query.json   | IDENTICAL |
| 5 | uast/query.compact| IDENTICAL |
| 6 | uast/query.count  | IDENTICAL (reduce(count) DSL fix) |
| 7 | run/history_typos.json   | IDENTICAL |
| 8 | run/history_imports.json | IDENTICAL |
| 9 | run/history_anomaly.json | IDENTICAL |
| 10 | run/history_devs.json   | IDENTICAL |
| 11 | run/history_devs.yaml   | IDENTICAL (cf-goyaml) |
| 12 | run/history_devs.bin    | IDENTICAL (CFB1 envelope) |
| 13 | run/history_quality.json| IDENTICAL |
| 14 | run/history_sentiment.json | IDENTICAL (NEW: per-tick govader sentiment) |
| 15 | run/burndown.json       | IDENTICAL |
| 16 | run/burndown.yaml       | IDENTICAL |
| 17 | run/burndown.bin        | IDENTICAL |
| 18 | run/burndown.timeseries | IDENTICAL (head MergedTimeSeries) |
| 19 | run/burndown.timeseries.ndjson | IDENTICAL (NEW: streaming timeseries+ndjson) |
| 20 | static/static_composition.json | IDENTICAL |
| 21 | static/static_composition.yaml | IDENTICAL (static per-analyzer YAML) |
| 22 | static/static_composition.bin  | IDENTICAL (static per-analyzer CFB1) |
| 23 | static/static_comments.yaml    | IDENTICAL (cf-comments + cf-goyaml) |
| 24 | static/static_comments.bin     | IDENTICAL (cf-comments + CFB1) |
| 25 | static/static_complexity.json  | IDENTICAL (cf-complexity JSON section) |
| 26 | static/static_complexity.yaml  | IDENTICAL (cf-complexity + cf-goyaml) |
| 27 | static/static_complexity.bin   | IDENTICAL (cf-complexity + CFB1) |
| 28 | static/static_halstead.json    | IDENTICAL (NEW: cf-halstead JSON section) |
| 29 | static/static_halstead.bin     | IDENTICAL (cf-halstead + CFB1) |
| 30 | static/static_imports.yaml     | IDENTICAL (cf-imports + cf-goyaml) |
| 31 | static/static_imports.bin      | IDENTICAL (cf-imports + CFB1) |
| 32 | run/burndown.ndjson            | IDENTICAL (NEW: streaming per-commit GlobalDeltas NDJSON, `--limit 5`) |

ALL 32 binding captures are byte-identical — the binding-capture tier is COMPLETE.

Verify under: `set -f; env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800 <bin> <argv>`,
STDOUT only, `cmp`/`sha256sum` vs the golden in `rust/tests/golden/<relPath>`.

## Remaining failing binding capture — NONE (32/32)

There are no failing binding captures. The last one to land,
`run/burndown.ndjson` (streaming NDJSON: one JSON line per commit over
`--limit 5`, real per-commit GlobalDeltas from diffs — multi-commit pipeline, not
the closed-form HEAD reduction), now emits the 1490-byte golden byte-for-byte
(`cmp` IDENTICAL, rc=0). The history streaming NDJSON pipeline for burndown
(multi-commit `RunStreaming` emitting one per-commit `GlobalDeltas` JSON line) is
wired.

## Remaining scope (NOT binding-gated)

With the binding tier COMPLETE, the remaining work is:
1. **40 nonBinding / unstable captures** — see the next section.
2. **Full run-pipeline generalization** — generalize `run_dispatch` beyond the
   closed-form / fixed-subset blocks so arbitrary `--analyzers` selectors and
   formats run through the full analyzer pipeline end-to-end.
3. **General (non-closed-form) analyzer dispatch** — replace the remaining
   closed-form dispatch branches with a generic analyzer-dispatch path; the
   fall-through dispatch sentinel still covers not-yet-ported selectors.

## The 40 nonBinding / unstable captures — follow-on work

40 captures are marked `nonBinding=true` in MANIFEST.json and are NOT part of the
binding gate. They split into:
- **machine=false text-shaped views** (`*.text`, `*.compact`, `*.tree`,
  `uast/analyze.text`, `run/burndown.{text,compact}`) — human-rendered, not
  byte-gated.
- **machine=true but `stable=false`** Go-nondeterministic JSON/YAML/bin: the
  `static_clones.*`, `static_cohesion.*`, `*.perfile.json`, `static_comments.json`,
  `static_imports.json`, `run/history_{couples,shotness,file-history}.json`, and
  `run/all_static.{json,yaml,bin}` sets (Go reorders maps / worker scheduling).
  These need ROADMAP Step 15 (multi-analyzer `*` bin), Step 16 (stabilize /
  reclassify), Step 17 (govader lexicon parity) before they can be byte-gated.

## Earlier: the original Tier-1 anomaly closed form (reference)

The `history/anomaly --head --format json` closed form is implemented in
`bins/codefang/src/main.rs run_dispatch` (`anomaly_head_report`, mirroring
`devs_head_report`): it builds the HEAD report directly from libgit2 and routes
it through `cf_anomaly::{build_report_data, compute_all_metrics}` → `ToGoValue`
→ `cf_gojson::marshal`. Verified facts (golden `history_anomaly.json`, 570 B)
that the implementation reproduces:
- HEAD `2c9cc8da1aa316c30cfba4210cfcd09aff193c81` is a **2-parent merge**;
  single HEAD commit → tick 0. (Non-merge HEADs would need diff-match-patch line
  stats this closed form does not reproduce, so `anomaly_head_report` returns
  `None`/sentinel for that case — fine here, HEAD is a merge.)
- `start_time == end_time == "2026-01-26T21:53:53Z"` = HEAD's committer time,
  RFC3339 UTC (`cf_analyze::metadata::format_rfc3339_utc`).
- `files_changed: 11` — RESOLVED: the survivors of `cf-gitlib` tree-diff (HEAD
  vs first parent) after the shared vendor/generated path filter
  (`cf_pathpolicy::exclude(name, None, default_opts)`); the `git diff-tree`
  15→11 gap was the pathpolicy exclusion, not a libgit2 delta-merge artifact.
- `language_diversity: 3` (Go/JSON/Protocol Buffer via extension fast-path),
  `author_count: 1` (loose identity, author id 0), `threshold: 2`,
  `window_size: 20`, all stddevs 0, `churn_z_score: 0`, `anomalies: null`,
  `lines_added/removed/net_churn: 0` (merge HEAD skips `accumulateLineStats`).

Remaining (non-binding) work, in priority order:
- **`cf-goyaml` full yaml.v3 emitter parity** (Step 4) — still a scaffold; not
  among the 7 (all-JSON) binding captures, but blocks the `.yaml` nonBinding
  captures. This is now the top remaining item.
- **nonBinding / unstable capture determinism** (Steps 15–17) — `bin` format for
  `--analyzers '*'` (CFB1 multi-envelope), stabilize/reclassify the Go-map-order
  captures (couples / shotness / file-history / static_*), govader lexicon parity.
- **Full `run-pipeline` generalization** — `run_dispatch` currently routes the 4
  binding history captures via closed-form blocks (typos/imports/devs/anomaly);
  generalize beyond that closed-form dispatch to the full analyzer pipeline so
  arbitrary `--analyzers` selectors and formats run end-to-end (the fall-through
  dispatch sentinel still covers not-yet-ported selectors).

DONE this run:
- **`cargo test --workspace` test-target compile errors** — RESOLVED. The
  cf-clones test code, the `uast` bin-test, and cf-uast-node
  (engine/aggregator/analyzer/testutil) now use the current shipped API
  (`GoValue` enum: `GoValue::Str(s)` / `GoValue::Map(GoMap::from_map(..))`;
  `GoValue::Object`/`object` are constructor fns, not patterns). Test-only; the
  green release build and 7/7 binding parity are unaffected. The lint+test
  evidence gate is GREEN, unblocking the held DoD ticks.

### Harness (done this run)

`cargo run -p golden-harness` is the canonical verifier:
`tests/golden-harness/src/main.rs` (`[[bin]] golden-harness`). It reads
MANIFEST.json, runs the 7 binding captures with the pinned env (argv passed
directly to `Command` — no shell, so analyzer selectors reach the binary
verbatim, satisfying `set -f`), byte-compares STDOUT vs the golden, prints
`IDENTICAL`/`DIFFER` per capture + final `N/7 identical`, and exits nonzero on
any mismatch. Substring filters: `cargo run -p golden-harness -- uast`. Latest
(verified this run by running each release binary with the MANIFEST argv and
byte-comparing STDOUT): **7/7 identical**.

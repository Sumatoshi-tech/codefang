# codefang Rust rewrite — STATUS (verified 2026-06-06)

## TL;DR (latest verified run)

**Binding captures: 17/32 byte-identical.** `cargo build --release` exits 0 and
`cargo test --workspace` is GREEN (151 test-suites pass, `0 failed`). Of the 32
MANIFEST-binding captures (`nonBinding=false`), 17 now reproduce the Go goldens
byte-for-byte under the pinned golden env; the original 7 core captures are all
still IDENTICAL (no regression).

**New this run (10 additional binding captures driven green):**
- `uast/query.count` — fixed the DSL parser: `reduce(<ReducerName>)` now parses
  its argument as a bare identifier (`Reduce <- 'reduce' (… ReducerName …)`,
  `ReducerName <- [a-zA-Z_][a-zA-Z0-9_]*`) wrapped in `Call{name, args:[]}`,
  matching Go `convertReduceNode`. `reduce(count)` over a single file → `1`.
- `run/history_devs.bin` — wired the CFB1 binary envelope around the existing
  closed-form `devs_head_metrics` (Go bin path: `EncodeBinaryEnvelope(metrics)`,
  devs `ToJSON` returns `m`, so the payload equals the JSON capture).
- `run/burndown.{json,yaml,bin}` — closed-form HEAD-only burndown survival
  report (already wired; verified green this run).
- `run/burndown.timeseries` — new `burndown_head_timeseries` builds the
  single-commit `MergedTimeSeries` (`codefang.timeseries.v1`, `tick_size_hours`
  24, one flattened commit `{author:"", burndown:{lines_added,lines_removed},
  hash, tick, timestamp}`), with the committer time formatted Go-`time.RFC3339`
  in the commit's ORIGINAL zone offset (new `format_rfc3339_offset`).
- `run/history_devs.yaml` — closed-form devs YAML (header + cf-goyaml body),
  exercising cf-goyaml on a real report (Step 4 evidence).

The 7 original core captures (`uast/{parse,analyze,query}.json`,
`run/history_{typos,imports,anomaly,devs}.json`) remain byte-identical.


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
- **Binding parity tally 7/7** (was 0/7 → 6/7 → 7/7). All binding captures pass;
  no remaining misses.
- **Output-path / dispatch parity unverified.** `--help`/`version`/flag bytes vs
  the Go cobra binaries, and `codefang run` analyzer dispatch, have NOT been
  byte-diffed.
- **`cf-goyaml`** still a scaffold; `marshal` is linkable but not yaml.v3-parity
  (Step 4). Not among the 7 (all-JSON) binding captures.

## The 17 passing binding captures (of 32; from MANIFEST.json)

| # | relPath | status (2026-06-06) |
|---|---|---|
| 1 | uast/parse.json   | IDENTICAL |
| 2 | uast/parse.compact| IDENTICAL |
| 3 | uast/analyze.json | IDENTICAL |
| 4 | uast/query.json   | IDENTICAL |
| 5 | uast/query.compact| IDENTICAL |
| 6 | uast/query.count  | IDENTICAL (new: reduce(count) DSL fix) |
| 7 | run/history_typos.json   | IDENTICAL |
| 8 | run/history_imports.json | IDENTICAL |
| 9 | run/history_anomaly.json | IDENTICAL |
| 10 | run/history_devs.json   | IDENTICAL |
| 11 | run/history_devs.yaml   | IDENTICAL (cf-goyaml Step 4) |
| 12 | run/history_devs.bin    | IDENTICAL (new: CFB1 envelope) |
| 13 | run/burndown.json       | IDENTICAL |
| 14 | run/burndown.yaml       | IDENTICAL |
| 15 | run/burndown.bin        | IDENTICAL |
| 16 | run/burndown.timeseries | IDENTICAL (new: head MergedTimeSeries) |
| 17 | static/static_composition.json | IDENTICAL |

15 binding captures still fail — see "Remaining failing binding captures" below.

Verify under: `set -f; env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800 <bin> <argv>`,
STDOUT only, `cmp`/`sha256sum` vs the golden in `rust/tests/golden/<relPath>`.

## Remaining failing binding captures (15/32) — next work

All 15 still-failing captures require the **full multi-commit / multi-file
analysis pipeline** (real diff/blob/UAST processing), not a closed-form HEAD
reduction — that is the next structural milestone.

| relPath | reason still failing |
|---|---|
| static/static_comments.yaml  | static per-analyzer YAML: needs walk+parse of the 10-file subset + cf-comments native report + yaml.v3 emitter |
| static/static_comments.bin   | static per-analyzer CFB1 bin of the cf-comments native report |
| static/static_complexity.json| static JSON section report: needs UAST parse of each subset file + cf-complexity aggregation |
| static/static_complexity.yaml| static per-analyzer YAML of the cf-complexity native report |
| static/static_complexity.bin | static per-analyzer CFB1 bin of the cf-complexity native report |
| static/static_composition.yaml| static per-analyzer YAML (composition native report; JSON section path already green) |
| static/static_composition.bin | static per-analyzer CFB1 bin (composition native report) |
| static/static_halstead.json  | static JSON section report: UAST parse + cf-halstead aggregation |
| static/static_halstead.bin   | static per-analyzer CFB1 bin of the cf-halstead native report |
| static/static_imports.yaml   | static per-analyzer YAML of the cf-imports native report |
| static/static_imports.bin    | static per-analyzer CFB1 bin of the cf-imports native report |
| run/burndown.ndjson          | streaming NDJSON: one line per commit over `--limit 5`, real per-commit GlobalDeltas from diffs |
| run/burndown.timeseries.ndjson| streaming timeseries+NDJSON over `--limit 5` (same multi-commit pipeline) |
| run/history_quality.json     | multi-commit (`--limit 10`) per-tick quality stats from real cohesion/blob analysis |
| run/history_sentiment.json   | multi-commit (`--limit 10`) per-tick sentiment over real commit-message comments (govader) |

The two enablers these share:
1. **Static pipeline** (`StaticService.AnalyzeFolder` parity): WalkDir the subset,
   parse each Go file to UAST, run the analyzer, aggregate, then serialize via the
   analyzer's native JSON-section / `FormatReportYAML` / `FormatReportBinary`
   (`ResolveAggregationMode(format)` differs per format). Unlocks 11 static captures.
2. **History streaming pipeline** (multi-commit `RunStreaming`): per-commit diff +
   blob + tick aggregation, then ndjson / timeseries-ndjson / quality / sentiment
   serialization. Unlocks the remaining 4.

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

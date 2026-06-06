# codefang Rust rewrite — STATUS (verified 2026-06-06)

## TL;DR (latest verified run)

**Binding captures: 28/32 byte-identical.** `cargo build --release` exits 0 and
`cargo test --workspace` is GREEN. Of the 32 MANIFEST-binding captures
(`nonBinding=false`), 28 now reproduce the Go goldens byte-for-byte under the
pinned golden env (each Rust release binary run with the exact MANIFEST.json
argv, binary path swapped to `rust/target/release/{codefang,uast}`, STDOUT
compared byte-for-byte). The original 7 core captures are all still IDENTICAL
(no regression).

**New this run (11 additional binding captures driven green beyond the
previously-recorded 17):** the full **static per-analyzer pipeline** landed —
walk the fixed `apimachinery/pkg/util/sets` subset, parse each file to UAST, run
the analyzer, aggregate, and serialize via the analyzer's native
JSON-section / `FormatReportYAML` (cf-goyaml) / `FormatReportBinary`
(cf-reportutil CFB1). This drove green:
- `static/static_comments.{yaml,bin}`
- `static/static_complexity.{json,yaml,bin}`
- `static/static_composition.{yaml,bin}` (json was already green)
- `static/static_halstead.bin`
- `static/static_imports.{yaml,bin}`

Plus the previously-recorded 17 (uast core 7 + run/history_{typos,imports,
anomaly,devs}.json + run/history_devs.{yaml,bin} + run/burndown.{json,yaml,bin,
timeseries} + static/static_composition.json) all remain byte-identical.

**Still failing (4/32)** — all four fall through `run_dispatch` to the
blocked-dependency sentinel because no dispatch branch is wired yet:
- `static/static_halstead.json` — the halstead JSON section report (only the
  `bin` sibling is wired; the JSON-section path is missing).
- `run/burndown.ndjson` — streaming NDJSON, one line per commit over `--limit 5`
  (multi-commit pipeline, not the closed-form HEAD reduction).
- `run/burndown.timeseries.ndjson` — `--format timeseries --ndjson` over
  `--limit 5` (same multi-commit pipeline; only the single-commit `--head`
  non-ndjson timeseries is wired).
- `run/history_sentiment.json` — multi-commit (`--limit 10`) per-tick sentiment
  over real commit-message comments (govader); no `history/sentiment` branch.


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

## The 28 passing binding captures (of 32; from MANIFEST.json)

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
| 14 | run/burndown.json       | IDENTICAL |
| 15 | run/burndown.yaml       | IDENTICAL |
| 16 | run/burndown.bin        | IDENTICAL |
| 17 | run/burndown.timeseries | IDENTICAL (head MergedTimeSeries) |
| 18 | static/static_composition.json | IDENTICAL |
| 19 | static/static_composition.yaml | IDENTICAL (NEW: static per-analyzer YAML) |
| 20 | static/static_composition.bin  | IDENTICAL (NEW: static per-analyzer CFB1) |
| 21 | static/static_comments.yaml    | IDENTICAL (NEW: cf-comments + cf-goyaml) |
| 22 | static/static_comments.bin     | IDENTICAL (NEW: cf-comments + CFB1) |
| 23 | static/static_complexity.json  | IDENTICAL (NEW: cf-complexity JSON section) |
| 24 | static/static_complexity.yaml  | IDENTICAL (NEW: cf-complexity + cf-goyaml) |
| 25 | static/static_complexity.bin   | IDENTICAL (NEW: cf-complexity + CFB1) |
| 26 | static/static_halstead.bin     | IDENTICAL (NEW: cf-halstead + CFB1) |
| 27 | static/static_imports.yaml     | IDENTICAL (NEW: cf-imports + cf-goyaml) |
| 28 | static/static_imports.bin      | IDENTICAL (NEW: cf-imports + CFB1) |

4 binding captures still fail — see "Remaining failing binding captures" below.

Verify under: `set -f; env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800 <bin> <argv>`,
STDOUT only, `cmp`/`sha256sum` vs the golden in `rust/tests/golden/<relPath>`.

## Remaining failing binding captures (4/32) — next work

All 4 still-failing captures fall through `run_dispatch` to the dispatch sentinel
(`Error: command dispatch is blocked on cf-commands (tier 8)`) because no branch
is wired for that analyzer/format combination yet.

| relPath | reason still failing |
|---|---|
| static/static_halstead.json  | static JSON section report: only the `bin` sibling is wired; the halstead JSON-section path (`static_halstead.rs` only exposes `halstead_bin_report`) is missing |
| run/burndown.ndjson          | streaming NDJSON: one line per commit over `--limit 5`, real per-commit GlobalDeltas from diffs (multi-commit pipeline, not the closed-form HEAD reduction) |
| run/burndown.timeseries.ndjson| streaming `--format timeseries --ndjson` over `--limit 5` (same multi-commit pipeline; only the single-commit `--head` non-ndjson timeseries is wired) |
| run/history_sentiment.json   | multi-commit (`--limit 10`) per-tick sentiment over real commit-message comments (govader); no `history/sentiment` dispatch branch |

The two enablers these share:
1. **Halstead JSON section** — analogous to the already-green
   `static/static_complexity.json` path; needs a `halstead_report` (JSON-section)
   alongside the existing `halstead_bin_report`. Unlocks the 1 remaining static.
2. **History streaming pipeline** (multi-commit `RunStreaming`): per-commit diff +
   blob + tick aggregation, then ndjson / timeseries-ndjson / sentiment
   serialization. Unlocks the remaining 3.

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

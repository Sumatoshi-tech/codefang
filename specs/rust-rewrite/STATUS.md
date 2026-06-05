# codefang Rust rewrite — STATUS (verified 2026-06-06)

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

- **`cargo test --workspace` does NOT compile.** The release **build** is green,
  but two TEST targets fail to compile: `cf-clones` (test code references stale
  `GoValue::Object` / `GoValue::str` / `GoValue::Str` and a wrong-arity call) and
  the `uast` bin test (1 error). These are test-only and do not affect the
  release binaries, but they block the lint+test evidence gate (so several
  build-blocker DoD boxes in ROADMAP.md are annotated "verified green" but left
  unticked until the gate passes).
- **Binding parity tally still 0/7.** Binaries run, but no binding JSON capture
  has been diffed against its golden yet. That is the next phase.
- **Output-path / dispatch parity unverified.** `--help`/`version`/flag bytes vs
  the Go cobra binaries, and `codefang run` analyzer dispatch, have NOT been
  byte-diffed.
- **`cf-goyaml`** still a scaffold; `marshal` is linkable but not yaml.v3-parity
  (Step 4). Not among the 7 (all-JSON) binding captures.

## The 7 binding captures (all JSON; from MANIFEST.json)

| # | relPath | binary + argv tail |
|---|---|---|
| 1 | uast/parse.json   | `uast parse --format json <byte.go>` |
| 2 | uast/analyze.json | `uast analyze --format json <byte.go>` |
| 3 | uast/query.json   | `uast query 'filter(.roles has "Function")' --format json <byte.go>` |
| 4 | run/history_typos.json   | `codefang run … --analyzers history/typos --format json --limit 10 --workers 1` |
| 5 | run/history_imports.json | `codefang run … --analyzers history/imports --format json --limit 10 --workers 1` |
| 6 | run/history_anomaly.json | `codefang run … --analyzers history/anomaly --format json --head --limit 5` |
| 7 | run/history_devs.json    | `codefang run … --analyzers history/devs --format json --head --limit 5` |

Verify under: `set -f; env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800 <bin> <argv>`,
STDOUT only, `cmp`/`sha256sum` vs the golden in `rust/tests/golden/<relPath>`.

## Exact next action

**Drive the 7 binding JSON captures above to byte-identical.** Order:
1. The 3 `uast` captures first (only need the UAST stack): `parse`, then
   `analyze`, then `query` (Steps 6→7→8).
2. The 4 `run` captures next (need git/pipeline/analyzer stack against
   `/home/dmitriy/sources/kubernetes`): `typos`, `imports`, `anomaly`, `devs`
   (Steps 9→10→11→12).
3. In parallel, fix the `cargo test --workspace` test-target compile errors
   (`cf-clones`, `uast` bin-test stale `GoValue` API) so `make lint`/`cargo test`
   go green and the build-blocker DoD boxes can be ticked.

For each capture: run the Rust binary under the golden env, capture STDOUT, diff
vs the golden; on mismatch, diff field-by-field (float formatting via cf-gojson
`ftoa`, map key sorting, HTML-escape, trailing newline are the usual suspects).

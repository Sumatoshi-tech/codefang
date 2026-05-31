# codefang Rust rewrite — STATUS (verified 2026-05-31)

Authoritative, evidence-backed snapshot. Companion docs: `ARCHITECTURE.md`,
`DESIGN.md`, `ROADMAP.md`, `rust/tests/golden/MANIFEST.json`.

## ✅ Green (verified on disk this session)

- **Tier-0 keystone `cf-gojson` — DONE.** `cargo test -p cf-gojson` = 19/19 pass.
  - `value.rs`: `GoValue` / `GoMap` / `MapOrigin` (struct = declaration order,
    map = byte-sorted on encode) + `GoMap::from_map`.
  - `marshal.rs`: Go `encoding/json` byte-parity — HTML-escape ON
    (`<`,`>`,`&`,U+2028/9 → `\u00xx`/` `/` `), byte-sorted map keys,
    `marshal` / `marshal_indent` / `Encoder` builder
    (`marshal`/`compact`/`encoder`/`indented` + `with_trailing_newline` +
    `encode`/`encode_to_vec`/`encode_to_string`).
  - `ftoa.rs`: Go shortest-float (`format_json_float`, `format_float_g`).
- **`cf-reportutil` builds** (consumes the keystone).
- **`cf-anomaly` — FULLY RECONSTRUCTED from Go** after the collision destroyed it;
  `cargo test -p cf-anomaly` = 9/9 pass. Files: `statistics.rs` (meanStd),
  `zscore.rs` (rollingZScores), `detect.rs` (DetectAnomalies), `model.rs` (fixed).
- `cf-version/src/lib.rs` duplicate-fn corruption — fixed.

## ⚠️ Concurrent-workflow corruption (root-caused this session)

Running multiple background Workflows while also hand-editing the same `rust/`
tree caused several files to be duplicated/destroyed. **`rust/` is not tracked in
git**, so there is no recovery net — corrupted files must be rewritten from the Go
source. Repaired: cf-version, cf-gojson tests, cf-anomaly (all 4 files). Still
corrupted: `cf-couples/src/{aggregator,lib}.rs`, `cf-uast-node/src/lib.rs`.

LESSON / PROCESS FIX: only ONE writer at a time. Either I edit directly OR a
single workflow runs — never both, and never two workflows on the same tree.

## ❌ Not done (blocks byte-identity verification)

1. **Full release build still RED.** `cf-analyze` has ~21 errors (unresolved
   `crate::{error,interfaces,report}`, `AggregatorOptions`, `FormatError`,
   `cf_alg_mapx`, `Clock`/`SystemClock`, `TimeSeriesError`, private `Aggregator`,
   and a call to `cf_goyaml::marshal` that doesn't exist). Mix of collision
   corruption and genuinely-missing items.
2. **`cf-couples` / `cf-uast-node`** have corrupted files (see above).
3. **`cf-goyaml`** is still a scaffold (Step 4) — needed by cf-analyze's YAML path
   (not by the 7 JSON binding captures, but it's referenced so it must at least
   expose `marshal`).
4. **CLI binaries** (`bins/{codefang,uast}/src/main.rs`) exist (~347 / ~241 LOC)
   but cannot build until cf-analyze compiles; `cf_version::{codefang,uast}_version_line`
   now exist (were duplicated, fixed).
5. **Byte-identity parity (Steps 6–12)** — the 7 binding JSON captures — pending a
   green build + binaries. 0/7 verifiable today.

## The 7 binding captures (all JSON; from MANIFEST.json)

| relPath | argv tail |
|---|---|
| uast/parse.json | parse --format json <byte.go> |
| uast/analyze.json | analyze --format json <byte.go> |
| uast/query.json | query 'filter(.roles has "Function")' --format json <byte.go> |
| run/history_typos.json | run … --analyzers history/typos --format json --limit 10 --workers 1 |
| run/history_imports.json | run … --analyzers history/imports --format json --limit 10 --workers 1 |
| run/history_anomaly.json | run … --analyzers history/anomaly --format json --head --limit 5 |
| run/history_devs.json | run … --analyzers history/devs --format json --head --limit 5 |

Verify under: `set -f; env TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800 <bin> <argv>`,
STDOUT only, `cmp`/`sha256sum` vs the golden.

## Exact next actions (single-writer, in order)

1. **Commit `rust/` to git** (safety net) before any further bulk edits.
2. Repair corrupted files from Go source: `cf-couples/{aggregator,lib}.rs`,
   `cf-uast-node/lib.rs`.
3. Make `cf-goyaml` expose `marshal(&GoValue) -> Vec<u8>` (real or stub) so
   cf-analyze links; then fix cf-analyze's remaining ~21 errors.
4. `cargo build --release` green → produce `target/release/{codefang,uast}`.
5. Drive the 7 binding JSON captures to byte-identical (Steps 6–12).
6. Implement `cf-goyaml` fully (Step 4); wire `golden-harness` (Step 14).

# Codefang Rust Rewrite — Final Design

> Status: authoritative design. Supersedes the `ARCHITECTURE.md` scaffold (which it
> incorporates and extends). This document is the byte-identity-first plan for porting
> codefang from Go to Rust while keeping libgit2.

---

## 0. The premise correction that reshapes serialization

There is **no LZ4 frame and no compression** anywhere in codefang's machine output.
`github.com/pierrec/lz4/v4 v4.1.22` appears in `go.mod` (line 81) but has **zero import
sites** in any `.go` file — it is an unused/transitive dependency. **Keep `lz4` out of the
Rust tree.**

The actual "bin" / "binary" machine format is the **CFB1 envelope**, defined in
`/home/dmitriy/sources/codefang/internal/analyzers/common/reportutil/binary.go`:

- 4 bytes ASCII magic `"CFB1"` (`BinaryMagic`, binary.go:17)
- 4 bytes little-endian `uint32` payload length (`binary.LittleEndian.PutUint32`, binary.go:42)
- N bytes payload = `json.Marshal(value)` — **compact, HTML-escape ON, no trailing newline** (binary.go:31)
- Multiple records are concatenated back-to-back; `DecodeBinaryEnvelopes` loops while bytes remain.

Therefore byte-identity for `bin` reduces to **byte-identity of compact Go JSON** plus an
8-byte header. The real adversary across every machine format is Go's `encoding/json`,
reproduced exactly. Everything below optimizes for that.

Ground-truth confirmed in this repo:

- `Report = map[string]any` (`internal/analyzers/analyze/analyzer.go:26`) ⇒ **map-key
  byte-order sorting is the dominant ordering rule**; struct field declaration order only
  governs wrapper types (`UnifiedModel`, `AnalyzerResult`, `MergedTimeSeries`, NDJSON line,
  `AnalysisMetadata`).
- Dispatch (`internal/analyzers/analyze/conversion.go`): JSON via `json.NewEncoder` +
  `SetIndent("", "  ")` (lines 305/312); YAML via `yaml.Marshal` (line 315); NDJSON via
  bare `json.NewEncoder` (line 343, compact).
- Format constants + `bin`→`binary` alias + `unsupported format: <fmt>` wording in
  `internal/analyzers/analyze/formats.go`.
- 68 `go-sitter-forest/*` grammars pinned at `v1.9.x` in `go.mod`.

---

## 1. Cargo workspace and crate-per-module layout

Single virtual workspace at `rust/Cargo.toml` (`[workspace]`, `resolver = "2"`), one crate per
Go package under `rust/crates/<crate>/`, two binaries under `rust/bins/`. The keystone is a
**new tier-0 crate with no Go counterpart: `cf-gojson`** (the Go-`encoding/json`
byte-compatible encoder), joined by `cf-goyaml` (the `gopkg.in/yaml.v3` byte-compatible
emitter). Every crate that emits a machine format depends on these, never on `serde_json` /
`serde_yaml` defaults for output.

```
rust/
  Cargo.toml                 # [workspace] members = ["crates/*", "bins/*"]
  rust-toolchain.toml        # pinned toolchain for reproducible float/sort behavior
  crates/
    # tier 0 — foundation + serialization compat (NEW crates land first)
    cf-gojson/   cf-goyaml/
    cf-version/  cf-safeconv/ cf-textutil/  cf-iosafety/  cf-meminfo/
    cf-metrics/  cf-pathfilter/ cf-units/   cf-pipeline/
    cf-alg/ cf-alg-bloom/ cf-alg-hashutil/ cf-alg-interval/ cf-alg-levenshtein/
    cf-alg-mapx/ cf-alg-stats/ cf-alg-cms/ cf-alg-hll/ cf-alg-minhash/ cf-alg-lsh/ cf-alg-lru/
    cf-uast-node/ cf-uast-mapping/ cf-uast-spec/ cf-uast-lsp/ cf-uast-uastmaps/
    cf-config/ cf-identity/ cf-observability/ cf-storage/ cf-persist/ cf-reportutil/
    cf-sentiment-lexicons/ cf-langpath/ cf-pathpolicy/ cf-checkpoint/
    cf-plotpage/ cf-terminal/ cf-spillstore/ cf-streaming/ cf-burndown-core/
    # tier 1-2
    cf-gitlib/ cf-plumbing/ cf-cache/ cf-uast/
    # tier 3-5
    cf-analyze/ cf-renderer/ cf-analyzers-common/ cf-analyzers-plumbing/ cf-framework/
    # tier 6-7 — concrete analyzers
    cf-budget/ cf-anomaly/ cf-analyzer-burndown/ cf-clones/ cf-cohesion/ cf-comments/
    cf-complexity/ cf-couples/ cf-devs/ cf-file-history/ cf-halstead/ cf-imports/
    cf-sentiment/ cf-shotness/ cf-typos/ cf-composition/ cf-quality/
    cf-mcp/                   # feature-gated `mcp`, not shipped by default
    # tier 8 — aggregation
    cf-commands/             # analyzer registration + clap command construction
  bins/
    codefang/  uast/
  golden/                    # dev-only byte-diff harness (see §6)
  tests/golden/              # captured goldens (per the golden manifest)
```

Notes:

- The two nested Go `go.mod`s (`pkg/uast`, `pkg/uast/uastmaps`) collapse into ordinary
  workspace crates — Rust needs no module nesting.
- `cf-textutil`, `cf-persist`, `cf-reportutil`, `cf-analyze`, `cf-renderer` depend on
  `cf-gojson` / `cf-goyaml`, not on serde defaults.
- `cf-uast` (the `uast` binary) does not depend on `cf-framework`, so it is the first
  end-to-end-shippable artifact (after tier 2).
- `cf-mcp` is behind a non-default Cargo feature `mcp` (the Go `mcp` command is build-ignored).

### 1.1 Port / PR order (aligned to the crate layout above)

1. **Tier 0 — foundation + serialization compat (FIRST):** `cf-gojson`, `cf-goyaml`, then
   `cf-version`, `cf-safeconv`, `cf-textutil` (wraps `cf-gojson`), `cf-persist` (JSON codec
   via `cf-gojson`; **drop gob**), `cf-reportutil` (CFB1 envelope), plus `cf-pipeline`,
   `cf-metrics`, `cf-pathfilter`, `cf-units`, `cf-alg-*`, `cf-uast-node` (exact `ToMap`),
   `cf-uast-mapping`, `cf-config`, `cf-identity`, etc.
2. **Tier 1-2:** sketches (`cf-alg-cms/hll/minhash` with bit-identical seeds), `cf-gitlib`
   (git2), `cf-plumbing`, `cf-cache`, `cf-uast`. **`uast` binary shippable here.**
3. **Tier 3-5:** `cf-analyze` (cross-format conversion hub — `conversion.go`, `timeseries.go`,
   `streaming_sink.go`, `formats.go`, `metadata.go` reimplemented over `cf-gojson`/`cf-goyaml`),
   `cf-renderer`, `cf-analyzers-common`, `cf-analyzers-plumbing`, `cf-framework`.
4. **Tier 6-7:** the 16 concrete analyzers (`clones`→`anomaly`→`sentiment` ordering per deps;
   `quality`/`composition` after their components).
5. **Tier 8:** `cf-commands` (registration + clap), `codefang` binary.

**Definition of done per crate:** its Layer-B golden (§6) passes byte-clean before the next
tier begins.

---

## 2. Byte-identity strategy for MACHINE formats

Machine formats requiring byte-identity: `json`, `yaml`, `ndjson`, `timeseries`,
`timeseries+ndjson`, `compact`, `bin`. Terminal/HTML (`text`, `plot`, `html`) are
**non-binding / cosmetic** (see §2.7).

### 2.1 Why serde_json + ryu cannot be used as-is

Four guaranteed byte-diffs versus Go `encoding/json`:

1. **Map key ordering.** Go sorts `map[string]X` keys by raw UTF-8 byte (`<`) order at encode
   time. `serde_json::Map` defaults to insertion order; even `preserve_order=off` does not
   fix the float/escape issues. We cannot delegate.
2. **HTML escaping ON by default.** Go's `json.Marshal` / `json.Encoder` escape `<`→`<`,
   `>`→`>`, `&`→`&`, and `U+2028`/`U+2029`→` `/` `. The repo never calls
   `SetEscapeHTML(false)` (zero hits). serde_json does no HTML escaping.
3. **Float formatting.** Go's json uses `strconv.AppendFloat(b, f, 'g', -1, 64)` semantics:
   switch to exponential when `exp < -4 || exp >= 21`, render exponent as `e±NN` with sign and
   at least two digits, and print integer-valued floats without a decimal point
   (`float64(1.0)` → `1`, `float64(1e21)` → `1e+21`). ryu/serde_json choose different
   exponent thresholds and a different exponent rendering (`1e21`, `1.0`). **Hand-written
   formatter required.**
4. **Trailing newline + compact-vs-indent semantics.** `json.Encoder.Encode` always appends
   exactly one `\n`; `json.Marshal` never does. Compact mode emits `{"a":1,"b":2}` (no space
   after `:`); indent mode emits `{\n  "a": 1,\n  "b": 2\n}` (space after `:`), with empty
   containers collapsed to `{}` / `[]`. serde_json differs on empty containers and the
   compact-mode colon spacing.

### 2.2 `cf-gojson` — the dedicated Go-compat serialization crate

A self-contained encoder (not a serde `Serializer` shim — serde's model cannot control
map-key sort + HTML-escape + Go-float at every needed point). Surface:

```rust
pub enum GoValue {
    Null, Bool(bool),
    Int(i64), Uint(u64),     // integers never go through the float path
    Float(f64),              // formatted via go_float, NOT ryu
    Str(String),
    Array(Vec<GoValue>),
    Object(GoMap),
}

// Dual-mode ordered container — the crux of the whole plan.
pub struct GoMap(Vec<(String, GoValue)>);
//   struct-origin object  -> field DECLARATION order preserved, honors json:"name,omitempty"
//   map-origin object      -> keys SORTED by key.as_bytes() at encode time

pub struct Encoder {
    indent: Option<&'static str>,  // None = json.Marshal (compact); Some("  ") = SetIndent("","  ")
    escape_html: bool,             // default true
    trailing_newline: bool,        // true for Encoder.Encode paths; false for Marshal paths
}
```

**The dual-mode `GoMap` is the single load-bearing rule.** Every *wrapper struct* is an
explicit fixed-order builder emitting fields in source declaration order (honoring
`omitempty`); every *dynamic report map* (`Report = map[string]any`, `Props
map[string]string`, the flattened `MergedCommitData`) is a `GoMap` the encoder **byte-sorts
immediately before writing**. This one rule reproduces:

- `MergedCommitData.MarshalJSON` (timeseries.go) flattening to `map[string]any` → byte-sorted
  keys interleaving `author`/`hash`/`tick`/`timestamp` with analyzer flag keys.
- `Node.ToMap()` (`pkg/uast/pkg/node/node.go`) → `children, id, pos, props, roles, token, type`
  in byte order.
- `Props map[string]string` byte-sorted.

**Go float formatter (`cf-gojson::go_float`)** reproduces `encoding/json`'s `floatEncoder`:

- non-finite → error (Go errors on NaN/Inf in json);
- `abs == 0` → `"0"` (preserving `-0.0` per Go);
- choose `'e'` when `exp < -4 || exp >= 21`, else `'f'` (the **21** threshold, not ryu's);
- shortest round-trip digits (Ryū/Grisu produce the same unique digit *sequence* as Go's
  `strconv` for f64) then **re-render** with Go's rules;
- exponent rendered `e` + sign + ≥2 digits (`1e+21`, `1.5e-05`); trailing zeros stripped.

Implementation: take the portable shortest-digit sequence from a Grisu/Ryū backend, then
render exponent-threshold/`e±NN`/integer-trimming ourselves. The millions-value differential
fuzz in §6 (Layer A) closes this.

**String escaping** reproduces `encodeState.string`: escape `"`, `\`, control chars as
`\u00XX` (with `\n \r \t` shortcuts), plus `<`, `>`, `&`, `U+2028`, `U+2029` when
`escape_html`; invalid UTF-8 → `�`.

**Indent writer** reproduces Go's `Indent`: compact = no spaces; indent = `{\n  "k": v\n}`
with one space after the colon; empty objects/arrays stay `{}` / `[]`.

### 2.3 Per-format encoder configuration (grounded in call sites)

| Format | Go call site | `cf-gojson`/`cf-goyaml` config |
| --- | --- | --- |
| `json` (run/render) | `conversion.go:305` `NewEncoder`; :312 `SetIndent("","  ")` | indent `"  "`, escape ON, trailing `\n` |
| `json` (textutil) | `pkg/textutil/textutil.go` `WriteJSON` | indent optional, escape ON, trailing `\n` |
| `yaml` | `conversion.go:315` `yaml.Marshal` | `cf-goyaml` (§2.4) |
| `ndjson` | `conversion.go:343` `NewEncoder` (no indent), `streaming_sink.go` | compact, escape ON, trailing `\n` **per line** |
| `timeseries` | `timeseries.go` `WriteMergedTimeSeries` (`SetIndent`) | indent `"  "`, escape ON, trailing `\n` |
| `timeseries+ndjson` | `timeseries.go` `WriteTimeSeriesNDJSON` | compact, escape ON, trailing `\n` per line |
| `compact` | `static.go` `FormatCompact` | compact, escape ON; line framing per `FormatCompact` |
| `binary`/`bin` | `reportutil/binary.go:31` `json.Marshal` payload | compact, escape ON, **no trailing newline**, wrapped in 8-byte CFB1 header |

`bin` normalizes to `binary` via `NormalizeFormat` (formats.go).

### 2.4 `cf-goyaml` — `gopkg.in/yaml.v3` byte-compatibility

YAML is harder than JSON: yaml.v3's emitter has many heuristics. Wrapper structs carry
explicit `yaml:"..."` tags (`metadata.go`, `conversion.go` `UnifiedModel`/`AnalyzerResult`).

- Reproduce **yaml.v3's emitter**, not a generic YAML lib. `serde_yaml`/`serde_yml` diverge on
  string-quoting heuristics, block-sequence indentation (yaml.v3 does **not** indent sequence
  items under a mapping key: `key:\n- item`), `null` vs `~`, and number rendering.
- **Key order:** struct fields in declaration order; `map` keys byte-sorted — reuse the dual
  `GoMap` ordering machinery from `cf-gojson`.
- **Scalar quoting:** port yaml.v3's `resolve` / `yaml_emitter_analyze_scalar` (plain vs single
  vs double quoting), so strings that look like numbers/bools/dates/`base60` get quoted exactly
  as Go does.
- **Floats:** port yaml.v3's own float rendering (distinct from the JSON one).
- **Indent width 2** for nested mappings; sequence dashes align with the key.

This is the **highest-residual-risk** machine target (§7).

### 2.5 The CFB1 "bin" record layout

`cf-reportutil` encodes each record as: `b"CFB1"` + `len(payload) as u32 LE`
(`u32::to_le_bytes`) + `payload`, where `payload = cf-gojson::Encoder{ indent: None,
escape_html: true, trailing_newline: false }.encode(value)`. Records concatenate. The decoder
loops `while remaining >= 8`, validates magic, reads the LE length, slices the payload. **No
compression, no LZ4.** (Mirrors `binary.go` exactly.)

### 2.6 Non-output byte-changers: enry and govader parity

These are not serialization, but they change *which/what* bytes appear in machine reports, so
they get the same identity discipline:

- **enry (`src-d/enry`) language classification.** Used by `cf-pathfilter`, `cf-langpath`,
  `cf-composition`. File inclusion / vendor / generated / language decisions select which data
  appears in output. **Port enry's data tables + heuristics verbatim** (do not swap to a
  different detector like hyperpolyglot). Golden over a mixed corpus.
- **govader (VADER, commit `f6505c8d`) sentiment.** Scores are floats in machine output. Port
  the **exact lexicon + scoring algorithm**, reusing `cf-sentiment-lexicons` embedded data.
  Fixed-corpus golden over `compound/pos/neu/neg`.
- **Sketch/hash determinism (HLL, MinHash, CMS, LSH, bloom).** Estimates are ints/floats in
  output. This is a faithful reimplementation of `cf-alg-hashutil` (Splitmix64, Mix64, fixed
  seeds), **bit-identical**, not a dependency swap. Golden hashes a fixed corpus and diffs vs Go.

### 2.7 Explicitly NON-BINDING (cosmetic) outputs

- **`jedib0t/go-pretty` tables (StyleLight)** → custom writer over `comfy-table`/`tabled`.
  Best-effort. `PadRight` / bar widths use **byte** length in Go (`cf-terminal` note) — match
  byte-width, not Unicode display width, but byte-identity is **not** required.
- **`go-echarts` + `html/template` HTML/plot** → `askama`/`minijinja` + echarts JS.
  Best-effort; byte-identity explicitly out of scope.
- `text` terminal output (`fatih/color`) → `anstyle`/`owo-colors`. Cosmetic.

### 2.8 Non-determinism / wall-clock hazards (neutralized, not "matched")

Byte-identity against a live clock is impossible; pin via injectable clock + env override
(mirroring Go's ldflags pattern), so goldens are reproducible on both sides:

- `AnalysisMetadata.AnalyzedAt = time.Now().UTC().Format(time.RFC3339)` (`metadata.go:23`):
  inject a `Clock` through `cf-analyze`; goldens set `CODEFANG_NOW` / `SOURCE_DATE_EPOCH`.
- `internal/analyzers/plumbing/ticks.go:163` `time.Now().Add(maxClockSkew)`: same injected clock.
- NDJSON `tc.Timestamp.Format(time.RFC3339)` (`streaming_sink.go`): commit-time-derived
  (deterministic given repo), but the formatter must match Go's `RFC3339`/`RFC3339Nano`
  (`Z` for UTC, `±HH:MM` offsets, trailing-zero fractional-second trimming). Implement
  `cf-gojson::rfc3339` — do **not** use `chrono`'s formatter (it differs on `Z` vs `+00:00`
  and fractional trimming).
- `cf-version` build metadata: `env!` / build script instead of ldflags; `built` date pinned
  via `SOURCE_DATE_EPOCH`.
- **Pre-existing Go map/slice nondeterminism** (cohesion/halstead function tables, file_history
  `Hashes`, shotness name collisions): for `map[string]any` paths `cf-gojson` byte-sorts
  deterministically; for slice paths where Go's order is itself nondeterministic, byte-identity
  is unachievable run-to-run on either side. The golden harness applies a **named
  canonicalizer** to those specific JSON paths on both Go and Rust before diff (§6), and these
  are filed as upstream determinism bugs rather than silently masked.

---

## 3. Dependency mapping (Go → Rust), keeping libgit2

| Go dependency | Rust replacement | Identity impact / notes |
| --- | --- | --- |
| `encoding/json` | **`cf-gojson` (custom)** | Core of the lens. Never `serde_json` for output. |
| `gopkg.in/yaml.v3` | **`cf-goyaml` (custom)** | Emitter ported, not `serde_yaml`/`serde_yml`. Highest-risk machine target. |
| `encoding/gob` | **drop** | Gob is Go-specific, not byte-portable. Checkpoint/persist on-disk state uses a Rust-native codec (`bincode`/`postcard`); never user-visible output. JSON codec path stays via `cf-gojson`. |
| `pierrec/lz4/v4` | **omit** | Unused in Go (zero import sites). |
| `encoding/binary` (LE u32) | `u32::to_le_bytes` (or `byteorder`) | CFB1 header only. |
| **git2go v34 / libgit2 1.5.0** | **`git2` crate (libgit2 bindings)** — KEEP libgit2 | Matches Go object hashing & diff exactly. Per-thread `Repository` (`!Send`/`!Sync`), RAII `Drop` replaces `Free()`, `blob.content().to_vec()`. Pin libgit2 1.5.x via the `third_party/libgit2` submodule + `git2`'s `vendored-libgit2` feature for identical diff/blob/hash semantics. |
| tree-sitter (`go-sitter-forest/*`, 68 langs) | `tree-sitter` crate + per-language grammar crates / vendored C | §5. Node positions/types flow into machine output → grammar versions pinned. |
| `src-d/enry` | **port enry data tables + heuristics** | §2.6. Classification changes report bytes. |
| `govader` (`f6505c8d`) | **port VADER lexicon + scoring** | §2.6. Scores are floats in output. |
| `spf13/cobra` | **`clap` (builder API)** | §4. Help/usage/error wording matched. |
| `spf13/viper` | `figment` or hand-rolled config | Precedence flag > env (`CODEFANG_`) > file > default, matched exactly. |
| `fatih/color` | `anstyle` / `owo-colors` | Terminal only — cosmetic. |
| `go-echarts` + `html/template` | `askama`/`minijinja` + echarts JS | Plot/HTML — **non-binding**. |
| `jedib0t/go-pretty` (StyleLight) | `comfy-table` / `tabled` (custom) | Terminal tables — **non-binding** (byte-width pad parity, best-effort). |
| otel + prometheus | `tracing` + `opentelemetry` + `metrics` | Behavioral parity; not in output bytes. |
| LSP server (`tower-lsp` implied) | `tower-lsp` | `uast lsp` behavioral parity. |
| embeds (`embedded_mappings.gen.go`, lexicons, schema) | `include_bytes!` / `include_str!` + build-time codegen | Regenerate `.uastmap` tables; do not hand-port (§5). |

---

## 4. CLI compat plan (clap reproducing cobra exactly)

Use **clap's builder API** (not derive) so command/flag declaration order, help text, and
error strings can be matched to cobra verbatim. Configure clap to disable color in non-TTY and
override the help template + error formatting to match cobra's `Usage:` / `Flags:` /
`Available Commands:` sections, two-space flag alignment, and the `Error: <msg>` then-usage
flow. Exit codes match cobra (`1` on `RunE` error → `process::exit(1)`).

**Critical error-handling asymmetry (must be reproduced):** `codefang` sets
`SilenceErrors=true` + `SilenceUsage=true` (`cmd/codefang/main.go`) → on error it prints only
`Error: %v\n` to STDERR + exit 1, NO usage. `uast` (`cmd/uast/main.go`) does NOT set those →
cobra prints usage+error. Configure clap per-binary accordingly: codefang suppresses the usage
block on runtime errors; uast emits it.

### 4.1 `codefang` binary (`cmd/codefang/main.go` + `cmd/codefang/commands/run.go`)

Root: `Use "codefang"`, Short `"Codefang Code Analysis - Unified code analysis tool"`.
Persistent flags `--verbose`/`-v`, `--quiet`/`-q`, `--profile` (all bool false). Subcommands
wired in `main.go`: **run, render, version**. `mcp` (`mcp.go`) has `//go:build ignore` and is
NOT added → feature-gated `mcp` in Rust, not shipped by default.

- **Malloc re-exec** (`ensureMallocTunables`, before parse): sets `MALLOC_ARENA_MAX=2`,
  `MALLOC_MMAP_THRESHOLD_=32768`, `MALLOC_TRIM_THRESHOLD_=16384`, `MALLOC_MMAP_MAX_=65536` then
  `syscall.Exec` self if `MALLOC_ARENA_MAX` is unset. Reproduce (set env + exec self) for memory
  parity, OR document as intentionally dropped. Not output-affecting.
- **`--profile` PersistentPreRun:** pprof HTTP `localhost:6060` + memory watchdog → optional
  `pprof`/`tracing` behind a feature; behavioral parity only.

`codefang version` (`Run`, exit 0): STDOUT `codefang %s (commit: %s, built: %s)\n` from
`pkg/version` (defaults `dev`/`none`/`unknown`, ldflags-injected). Reproduce byte-for-byte via
`env!`/build-script constants; `built` pinned via `SOURCE_DATE_EPOCH`. No flags.

`codefang run [path]` — Short `"Run static and history analyzers"`, Long `"Run selected static
and history analyzers."`, `Args = MaximumNArgs(1)`. **Literal flags** (confirmed
`run.go:268-320, 792-800`; reproduce each long name, short, type, default, and help string
verbatim):

| Flag | Short | Type | Default | Notes |
| --- | --- | --- | --- | --- |
| `--analyzers` | `-a` | []string | nil | "Analyzer IDs or glob patterns (example: static/complexity,history/*,*)" |
| `--format` | | string | `json` | "Output format: json, yaml, plot, bin, timeseries, ndjson, text, compact" |
| `--ndjson` | | bool | false | with `--format timeseries` → composes `timeseries+ndjson` |
| `--input` | | string | `""` | cross-format conversion input |
| `--input-format` | | string | `auto` | auto, json, bin |
| `--gogc` | | int | 0 | Go-runtime-specific; map or drop |
| `--ballast-size` | | string | `0` | Go-GC-specific; map or drop |
| `--silent` | | bool | false | disable progress |
| `--no-color` | | bool | false | disable colored static output |
| `--path` | `-p` | string | `.` | positional `[path]` overrides this |
| `--debug-trace` | | bool | false | |
| `--cpuprofile` / `--heapprofile` | | string | `""` | |
| `--limit` | | int | 0 | 0 = no limit |
| `--first-parent` / `--head` | | bool | false | |
| `--since` | | string | `""` | "e.g., '24h', '2024-01-01', RFC3339" |
| `--workers` / `--static-workers` | | int | 0 | |
| `--include-vendored` / `--include-generated` | | bool | false | multi-sentence help — copy verbatim |
| `--extra-excluded-prefixes` | | []string | nil | |
| `--per-file` | `-F` | bool | false | |
| `--buffer-size` / `--commit-batch-size` | | int | 0 | |
| `--blob-cache-size` / `--blob-arena-size` / `--memory-budget` | | string | `""` | |
| `--diff-cache-size` | | int | 0 | |
| `--max-changes-per-commit` | | int | 0 | multi-sentence help — copy verbatim |
| `--checkpoint` | | bool | **true** | tri-state via `Changed()` → `Option<bool>` |
| `--checkpoint-dir` | | string | `""` | |
| `--resume` | | bool | **true** | tri-state via `Changed()` → `Option<bool>` |
| `--clear-checkpoint` | | bool | false | |
| `--cache-dir` | | string | `""` | |
| `--no-cache` | | bool | false | |
| `--config` | | string | `""` | ".codefang.yaml in CWD or $HOME" |
| `--list-analyzers` | | bool | false | prints to STDOUT, exit 0 |
| `--diagnostics-addr` | | string | `""` | |
| `--output` | `-o` | string | `""` | "required with --format plot" |
| `--keep-store` | | bool | false | |
| `--tmp-dir` | | string | `""` | |

- **Tri-state `--checkpoint`/`--resume`** (both default true) read via `Flags().Changed(name)`
  so config/file defaults apply only when the CLI flag was NOT supplied → model as
  `Option<bool>` / clap value-source detection, not a plain bool.
- **Deprecated (keep hidden, exact messages):** `--skip-blacklist` ("use --include-vendored=false
  and --include-generated=false (the new defaults). See CHANGELOG for migration."),
  `--blacklisted-prefixes` ("use --extra-excluded-prefixes; the old flag name is preserved for
  back-compat but will be removed in the next minor release.").
- **Dynamic per-analyzer flags** (`registerAnalyzerFlags`): iterates every analyzer's
  `ListConfigurationOptions()` and registers one flag per option (Bool/Int/String/StringSlice/
  Float64/Path), including `--languages` ([]string). In Rust `cf-commands` builds the clap
  `Command` from the same analyzer registry so names/defaults/help match. Exact strings are only
  fully knowable by dumping `codefang run --help` at runtime — Layer-D golden (§6) pins them.
- **`--format` validation** routes through reimplemented `NormalizeFormat`/`ValidateFormat`/
  `ValidateUniversalFormat` (formats.go): `bin`→`binary` alias, `unsupported format: <fmt>`
  wording, and `--ndjson` + `--format timeseries` → `timeseries+ndjson` composition.
- **Sentinel errors (exact wording):** `ErrNoAnalyzersSelected` ("no analyzers selected. Use -a
  flag, e.g.: -a burndown,couples"), `ErrUnknownAnalyzer`, `ErrRepositoryLoad` ("failed to load
  repository"), `ErrPlotOutputRequired` ("--output flag is required when --format plot").

`codefang render <store-dir>` — Short `"Render stored analysis results as multi-page HTML"`,
`Args = ExactArgs(1)`, flag `--output`/`-o` string `""`. Sentinels: `ErrNoOutputDir` ("output
directory is required (use --output)"), `ErrEmptyStore` ("no analyzer data found in store"),
`ErrNoSectionRenderer` ("no section renderer registered"). Writes HTML + `report.json` (mode
0640). (HTML output is non-binding cosmetic; `report.json` uses `WriteJSON(pretty=true)`.)

### 4.2 `uast` binary (`cmd/uast/main.go` + `cmd/uast/*.go`)

Root: `Use "uast"`, Short `"UAST (Universal Abstract Syntax Tree) parser and analyzer"`.
Persistent flags `--config` string `""`, `--verbose`/`-v` bool, `--quiet`/`-q` bool. **Does NOT
set `SilenceErrors`/`SilenceUsage`** → emit usage on error (asymmetry vs codefang). 11
subcommands wired in `main.go` (`parseCmd … serverCmd`), each in its own `cmd/uast/<name>.go`:

| Command (`Use`) | Args | Flags (long/short default; valid values) | Output / exit |
| --- | --- | --- | --- |
| `version` | — | — | STDOUT `uast %s (commit: %s, built: %s)\n`; exit 0 |
| `parse [files...]` | variadic (stdin if none) | `--language`/`-l` "", `--output`/`-o` "", `--format`/`-f` `json` (json,compact,tree,none), `--progress`/`-p` false, `--all` false, `--workers`/`-w` 0 | STDOUT or `-o`; `ErrUnsupportedParseFmt`, `ErrNoSourceFiles` |
| `diff file1 file2` | ExactArgs(2) | `--output`/`-o` "", `--format`/`-f` `unified` (unified,summary,json) | `ErrUnsupportedFileType`, `ErrUnsupportedDiffFmt` |
| `query [query] [files...]` | manual (`ErrQueryExprRequired` if 0) | `--input`/`-i` "", `--output`/`-o` "", `--format`/`-f` `json` (json,compact,count), `--interactive`/`-t` false | stdin if no files & no `--input`; `ErrUnsupportedQFmt` |
| `explore [file]` | manual (`ErrNoFileSpecified`) | `--language`/`-l` "" | REPL → STDOUT; `ErrUnsupportedExploreFile` |
| `analyze [files...]` | manual (`ErrNoFilesSpecified`) | `--output`/`-o` "", `--format`/`-f` `text` (text,json,html) | `ErrUnsupportedAnaFmt` |
| `completion [shell]` | ExactArgs(1) | — | bash/zsh/fish/powershell → STDOUT; `ErrUnsupportedShell` |
| `validate <file.json\|->` | ExactArgs(1) | `--schema` "pkg/uast/spec/uast-schema.json", `--color` false, `--no-color` false | **exit 0/1/2 via `os.Exit`** (see below) |
| `mapping` | variadic | `--node-types` "", `--mapping` "", `--format` `text` (text,json), `--coverage` false, `--generate` false, `--show-treesitter` false, `--language` "", `--extensions` "" | `ErrNodeTypesRequired`, `ErrNoInputFiles`, `ErrNoRootNode`, `ErrUnsupportedLanguage` |
| `lsp` | — | — | LSP over stdio |
| `server` | — | `--port`/`-p` "8080", `--static`/`-s` "" | HTTP on `:PORT` |

- **`uast validate` exit codes (via `os.Exit`, not cobra):** valid → 0; validation FAILED → 1;
  bad JSON / schema read / open / engine error → 2. `--no-color` wins over `--color`.
- The machine-binding `uast` paths in the golden manifest are `parse`/`analyze`/`query` with
  `--format json` (their YAML/text captures are non-binding). Each is reproduced with the exact
  flag set above under the clap builder + cobra-mirroring help/error template. `completion`
  scripts mirror cobra's per-shell output. `version` byte-for-byte.

---

## 5. tree-sitter language set

68 grammars via `go-sitter-forest/*` at `v1.9.x` (ansible, bash, c, c_sharp, clojure, cpp,
crystal, css, dart, dockerfile, elixir, elm, fortran, git_config, go, graphql, groovy, haskell,
hcl, helm, html, ini, java, javascript, json, kotlin, latex, lua, make, markdown, nim, perl,
php, powershell, properties, proto, prql, python, r, rego, ruby, … pinned per-language in
`go.mod`).

- Use the upstream **`tree-sitter` Rust crate**; each language is a grammar crate
  (`tree-sitter-bash`, `tree-sitter-python`, …). For long-tail grammars that go-sitter-forest
  vendors but lack a Rust crate, **vendor the C sources** (`parser.c` [+ `scanner.c`]) into
  `cf-uast/grammars/` and compile via the `cc` crate, exposing `extern "C" fn
  tree_sitter_<lang>()`. go-sitter-forest is CGO over the same C, so parse trees are identical
  node-for-node — critical because UAST node positions/types are in machine output.
- **Pin each grammar to the exact upstream commit `go-sitter-forest v1.9.x` vendored.** A
  grammar bump changes node types → changes UAST → changes output bytes.
- **Regenerate, don't hand-port:** a `build.rs` emits the language `GetLanguage` dispatch and
  the `.uastmap` mapping tables (replacing the 2.75 MB `embedded_mappings.gen.go`) into
  `OUT_DIR`, embedded via `include_bytes!`.
- The **mapping DSL + native tree-sitter query/capture compiler** (`uast-mapping`,
  `pattern_matcher.go`) is reimplemented in Rust (tree-sitter does not provide it).
- Parser pool + lazy loader + bloom membership (`cf-uast`): reproduce loader semantics; the
  bloom filter is internal (behavior only, not output).
- Per-language golden (§6 Layer C) parses a fixture corpus and diffs `ToMap` JSON to catch
  grammar drift.

---

## 6. Golden-diff integration-test harness

Dev-only crate `rust/golden/` + fixtures + the captured goldens under
`rust/tests/golden/`. The harness **diffs only the binding (machine) goldens as hard
gates**; human-format diffs are reported **informationally** (never fail CI). A `MANIFEST.json`
drives it: each record carries `command`, `argv`, `outPath`, `format`, `sha256`, `machine`
(binding?), `nonBinding`, and an optional per-record **comparison mode** (raw vs canonicalized).

**Binding goldens (hard gate, raw-byte `assert_eq!`)** — from the manifest, e.g.:
`run --analyzers history/anomaly --format json` (570 B),
`history/devs --format json` (831 B), `history/imports --format json --workers 1` (167 B),
`history/typos --format json --workers 1` (138 B), plus the `uast parse/analyze/query --format
json` captures. Both the Go reference binary (current build) and the Rust binary run with
`CODEFANG_NOW`/`SOURCE_DATE_EPOCH`/`TZ`/`NO_COLOR`/`LANG`/`LC_ALL` pinned and a pinned
repo+blob, so wall-clock hazards are neutralized.

**Non-binding records (informational only):** the 18 captures the manifest marks `nonBinding`
(burndown.{json,yaml,timeseries,bin,ndjson}, run.all.*, several history/static analyzers,
`*.text/plot/html/compact`, `uast.*.yaml`, `uast.analyze.text`) — reasons: Go map-order
nondeterminism (DIFFER on Go-to-Go rerun), unsupported format (compact/html rc=1),
human/dir-output, or yaml-ignored. These are diffed and **printed as a report**, not gated.

Layers:

- **Layer A — `cf-gojson`/`cf-goyaml` unit differential tests.** A one-off Go helper
  (`golden/gen/main.go`, built with the real `encoding/json` / `yaml.v3`) emits canonical Go
  bytes for a large adversarial corpus to `golden/data/{json,yaml}/*.golden`; the Rust test
  feeds the same logical values through `cf-gojson`/`cf-goyaml` and asserts byte equality.
  Adversarial on the four divergences:
  - **Floats:** property/fuzz over random `f64` (subnormals, `1e-5`, `1e20`, `1e21`, `1e-4`,
    integer-valued floats, `-0.0`, extreme exponents) — **millions** of values vs Go
    `strconv.AppendFloat('g',-1,64)`. Highest-value test under the lens.
  - **Strings:** all control chars, `< > &`, `U+2028`/`U+2029`, invalid UTF-8, surrogate edges.
  - **Map key ordering:** byte-vs-rune ordering, empty keys, prefix keys.
  - **Indent vs compact, empty containers, trailing-newline presence.**
- **Layer B — golden output per analyzer × machine format.** Hermetic fixture repos (fixed
  commit hashes via pinned `GIT_*_DATE`); run the Go binary, then the Rust binary, `assert_eq!`
  raw bytes (hexdump on mismatch). For `bin`, split CFB1 envelopes and diff each compact-JSON
  payload separately to localize failures. **This is the per-crate DoD gate.**
- **Layer C — UAST parse golden.** Per-language corpus → `uast parse --format json`; diff
  `Node.ToMap()` Go-vs-Rust to catch grammar drift and ordering bugs.
- **Layer D — CLI golden.** Capture stdout/stderr/exit-code of `--help`, bad-flag, bad-format,
  `version` for both binaries and every subcommand; diff bytes — enforces clap-mirrors-cobra.

**Named canonicalizers** for legitimate Go-side nondeterminism (cohesion/halstead function
tables, file_history `Hashes`, shotness collisions): sort the specific JSON path on both Go and
Rust before diff; the record explicitly declares the canonicalized path so a real ordering
regression is never masked. Everything else is raw-byte.

---

## 7. Top residual risks

- **`cf-goyaml` YAML emitter identity** — highest. yaml.v3 scalar-quoting + float heuristics are
  intricate. Mitigation: Layer-A differential corpus vs a Go yaml.v3 generator; narrow residual
  heuristics with targeted fuzz seeds; fallback is to document divergent scalar classes rather
  than ship a silent diff.
- **Go float rendering edge cases** (`'g'/-1`, `e±NN`). Mitigation: millions-value Layer-A
  fuzz; tractable because the shortest-digit sequence is portable and only rendering differs.
- **tree-sitter grammar drift** — a single bump changes output bytes. Mitigation: pin to
  go-sitter-forest v1.9.x commits; Layer-C per-language golden.
- **enry classification parity** — changes which data appears. Mitigation: port enry data tables
  verbatim; golden over a mixed corpus.
- **VADER / sketch numeric parity** — scores/estimates are output. Mitigation: bit-identical
  reimplementation + fixed-seed golden.
- **libgit2 version pinning** — diff/blob/hash must match git2go's libgit2 1.5.0. Mitigation:
  vendored libgit2 1.5.x via submodule + `git2 vendored-libgit2`.
- **Map-origin vs struct-origin misclassification** — sorting fields that should stay in
  declaration order, or vice versa, silently corrupts ordering. Mitigation: every wrapper is an
  explicit fixed-order `GoMap` builder; every dynamic report is a sort-on-encode `GoMap`;
  Layer-B golden catches misclassification immediately.

---

## Appendix — grounding files (absolute paths)

- `/home/dmitriy/sources/codefang/internal/analyzers/common/reportutil/binary.go` — CFB1
  format (magic 17, `json.Marshal` payload 31, LE u32 42); **no LZ4/compression**.
- `/home/dmitriy/sources/codefang/internal/analyzers/analyze/analyzer.go:26` —
  `Report = map[string]any`.
- `/home/dmitriy/sources/codefang/internal/analyzers/analyze/conversion.go` — JSON
  (NewEncoder 305 / SetIndent "  " 312), YAML (yaml.Marshal 315), NDJSON (NewEncoder 343).
- `/home/dmitriy/sources/codefang/internal/analyzers/analyze/timeseries.go` —
  `MergedCommitData.MarshalJSON` flatten→map (byte-sorted), `WriteMergedTimeSeries`,
  `WriteTimeSeriesNDJSON`.
- `/home/dmitriy/sources/codefang/internal/analyzers/analyze/streaming_sink.go` — NDJSON line +
  `time.RFC3339`.
- `/home/dmitriy/sources/codefang/internal/analyzers/analyze/formats.go` — format constants,
  `bin`→`binary` alias, `unsupported format` wording.
- `/home/dmitriy/sources/codefang/internal/analyzers/analyze/metadata.go` — `AnalyzedAt` wall
  clock; wrapper field order.
- `/home/dmitriy/sources/codefang/pkg/uast/pkg/node/node.go` — `ToMap()` byte-sorted keys.
- `/home/dmitriy/sources/codefang/cmd/uast/main.go` — uast root + the 11 subcommand
  constructors (`parseCmd … serverCmd`); does NOT set `SilenceErrors`/`SilenceUsage`.
- `/home/dmitriy/sources/codefang/cmd/uast/parse.go` (and `diff.go`, `query.go`, `explore.go`,
  `analyze.go`, `validate.go`, `mapping.go`, `completion.go`, `lsp.go`, `server.go`) — per-subcommand
  flag sets, defaults, valid format values, and sentinel errors.
- `/home/dmitriy/sources/codefang/cmd/codefang/main.go` — codefang root: `SilenceErrors`/
  `SilenceUsage`, `ensureMallocTunables` re-exec, `--profile` watchdog, run/render/version wiring.
- `/home/dmitriy/sources/codefang/cmd/codefang/commands/run.go` — `run [path]`: the ~45 literal
  flags (lines 268-320, 792-800), tri-state `--checkpoint`/`--resume`, deprecated flags,
  `registerAnalyzerFlags` dynamic per-analyzer flags, sentinel errors.
- `/home/dmitriy/sources/codefang/internal/analyzers/plumbing/ticks.go:163` —
  `time.Now().Add(maxClockSkew)` wall clock.
- `/home/dmitriy/sources/codefang/pkg/textutil/textutil.go` — `WriteJSON`.
- `/home/dmitriy/sources/codefang/pkg/persist/codec.go` — `JSONCodec` + `GobCodec` (drop gob).
- `/home/dmitriy/sources/codefang/go.mod` — 68 `go-sitter-forest` grammars at v1.9.x;
  `pierrec/lz4/v4` line 81 present but unused.
- `/home/dmitriy/sources/codefang/.gitmodules` — `third_party/libgit2` submodule (pin source
  for `git2` vendored build).

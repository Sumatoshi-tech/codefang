# cf-uast-uastmaps

Embedded per-language UAST (`.uastmap`) mapping tables for Codefang.

Rust port of the Go "uastmaps" data module: the `pkg/uast/uastmaps` directory of
`.uastmap` files plus the generated `pkg/uast/embedded_mappings.gen.go`, which
exposes a single `var EmbeddedMappings map[string]string` keyed by language name.

This crate embeds **68** per-language mapping files at compile time and exposes a
small accessor API (`embedded_mappings`, `get`, `contains`, `supported_languages`,
`len`, `is_empty`, `EMBEDDED_MAPPING_COUNT`).

## Usage

```rust
// Look up one language's verbatim `.uastmap` content.
let go = cf_uast_uastmaps::get("go").expect("go is embedded");
assert!(go.starts_with("[language \"go\""));

// Or enumerate every embedded language (sorted, UTF-8 byte order).
let langs = cf_uast_uastmaps::supported_languages();
assert!(langs.contains(&"rust"));
```

This snippet is the compile-checked doctest on
[`get`](src/lib.rs) / [`supported_languages`](src/lib.rs).

## Data provenance

The `mappings/*.uastmap` files are **vendored byte-for-byte** from the Go module
(`pkg/uast/uastmaps/*.uastmap`), so the mapping content the Rust UAST pipeline
consumes is identical to the Go implementation's.

## Generated embedding table — do not hand-edit

The Go module's embedding is produced by `tools/uastmapsgen/gen_uastmaps.py`
(`make uastmaps-gen`), which writes the ~33 MB `embedded_mappings.gen.go`. We do
**not** hand-translate that artifact. Instead `build.rs` is the Rust port of that
generator: it globs `mappings/*.uastmap` (sorted, exactly like the Python
`sorted(glob.glob(...))`), derives each language name as the filename stem, and
emits `$OUT_DIR/mappings.gen.rs` — a static table built with `include_str!`, the
direct analogue of the Go generator's string-literal embedding. (The Python
generator's 60-char chunking is cosmetic source formatting only and has no effect
on the runtime string value.)

To add or update a language mapping:

1. Add/replace `mappings/<lang>.uastmap`.
2. Rebuild. `build.rs` re-runs automatically (it declares `cargo:rerun-if-changed`
   on the directory and every file) and regenerates the table.
3. Update the `EXPECTED` list in the tests if the language set changed.

## Determinism

Go map iteration is randomized; the only order-sensitive Go consumer
(`SupportedLanguages`) sorts. This crate backs the data with a `BTreeMap`, so
iteration is inherently sorted (UTF-8 byte order) and every listing is
deterministic.

## Tests

The Go `pkg/uast/uastmaps` directory contains only data (no `.go` source, no Go
tests), so there are no Go tests to port. The unit tests here assert the embedded
set (count, exact language list, sorted order), that every mapping is non-empty,
the `get`/`contains` accessors, and that each embedded value is byte-identical to
its vendored source file. The `Loader` / `SupportedLanguages` behavior that wraps
this data on the Go side lives in `pkg/uast` (the `cf-uast` crate), which should
depend on this crate.

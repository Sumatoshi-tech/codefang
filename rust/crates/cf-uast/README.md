# cf-uast

The aggregate UAST (Universal Abstract Syntax Tree) parser facade for Codefang.

It is the single entry point for turning a source file into a UAST: detect
whether a file is supported, resolve its language, parse it into a
[`Node`](../cf-uast-node) tree, and diff two trees structurally. It wires
together the sibling crates so callers depend on one crate instead of four:

- [`cf-uast-node`](../cf-uast-node) — the `Node` tree and its byte-sorted
  `to_map` serialization.
- [`cf-uast-mapping`](../cf-uast-mapping) — the mapping DSL parser and native
  tree-sitter query/capture compiler.
- [`cf-uast-mappings`](../cf-uast-mappings) — the Rust-native mapping tables
  (system of record) the loader builds rules from.
- [`cf-uast-uastmaps`](../cf-uast-uastmaps) — the embedded `.uastmap` text
  tables (used by the dev server's text endpoints).

This crate deliberately does **not** depend on `cf-framework`, so the `uast`
binary stays the first end-to-end-shippable artifact.

## Usage

```rust
use cf_uast::Parser;

let parser = Parser::new();

// Detect support and resolve the language from a filename.
assert!(parser.is_supported("main.go"));
assert_eq!(parser.get_language("lib.rs"), "rust");

// A filename with no extension is unsupported.
assert!(!parser.is_supported("Makefile"));
```

This snippet is the compile-checked doctest on
[`Parser`](src/parser.rs). `Parser::parse` returns a `Node` tree; to emit a
MACHINE-format report from it, build the node's map-origin `GoValue` with
[`Node::to_map`](../cf-uast-node) and encode it with the `cf-gojson`
marshaller — never `serde_json` (the report-format byte-identity contract).

## Build

```sh
cargo build -p cf-uast
cargo test -p cf-uast
```

The build compiles the vendored tree-sitter grammar C sources under `vendor/`
via `build.rs`; see `src/languages.rs` for the language dispatch.

## Further reading

- Crate API docs: `cargo doc -p cf-uast --open`.
- Byte-identity / report-format contract: `specs/rust-rewrite/DESIGN.md` §2.
- Parity gate: `rust/tests/compat`.

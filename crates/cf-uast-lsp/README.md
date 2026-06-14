# cf-uast-lsp

A Language Server Protocol (LSP) server for the UAST mapping DSL (`.uastmap`
files), built on [`tower-lsp`](https://docs.rs/tower-lsp).

It powers the `uast lsp` subcommand and gives editors completion, hover, and
document tracking for the mapping DSL. The server emits only LSP protocol
JSON-RPC, which is an editor wire protocol — not a Codefang MACHINE-format
report — so the project's `cf-gojson` report-serialization rule does not apply
here.

## Usage

Run the server over stdio (the `uast lsp` path):

```no_run
# async fn run() {
cf_uast_lsp::run_stdio().await;
# }
```

The building blocks are also public and unit-/doc-tested independently:

```rust
use cf_uast_lsp::{all_completions, extract_word_at_position, hover_doc};

// The word under a cursor (byte-offset positions; `<-` is a single word).
assert_eq!(extract_word_at_position("rule <- pattern", 0, 6), "<-");

// Static completion items: keywords first, then UAST fields.
let labels: Vec<String> = all_completions().into_iter().map(|i| i.label).collect();
assert_eq!(&labels[..3], &["<-", "=>", "uast"]);

// Hover documentation for a keyword.
assert!(hover_doc("<-").unwrap().contains("pattern"));
```

The second snippet is the compile-checked doctest set on
[`extract_word_at_position`](src/text.rs), [`all_completions`](src/completion.rs),
and [`hover_doc`](src/completion.rs).

## Build

```sh
cargo build -p cf-uast-lsp
cargo test -p cf-uast-lsp
```

## Further reading

- Crate API docs: `cargo doc -p cf-uast-lsp --open`.
- The DSL the server assists with: [`cf-uast-mapping`](../cf-uast-mapping).

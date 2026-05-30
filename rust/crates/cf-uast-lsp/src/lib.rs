//! `cf-uast-lsp` — Language Server Protocol (LSP) server for the UAST mapping DSL.
//!
//! This crate is the Rust port of the Go package `pkg/uast/lsp` (purpose: "LSP
//! server over mapping/query DSL (uast lsp); behavioral parity via tower-lsp;
//! used by `cmd/uast`"). It provides editor support — completion, hover, and
//! document tracking — for the `.uastmap` mapping DSL used by the UAST framework.
//!
//! # Mapping from the Go implementation
//!
//! The Go server is built on the `tliron/glsp` stack. Per the Rust-rewrite
//! design (DESIGN §3, row "LSP server (tower-lsp implied) -> tower-lsp"), this
//! port uses [`tower_lsp`]. The pieces are split across modules:
//!
//! * [`document_store`] — the thread-safe URI→content store (Go `DocumentStore`).
//! * [`text`] — cursor/word helpers (`extractWordAtPosition`, `isWordChar`,
//!   `splitLines`), ported with Go's byte-offset semantics.
//! * [`completion`] — the static keyword/field completion items and hover docs.
//! * [`backend`] — the [`tower_lsp::LanguageServer`] impl (Go `Server` + its
//!   `protocol.Handler` wiring).
//!
//! # Serialization note (byte-identity)
//!
//! This server emits **only LSP protocol JSON-RPC messages**, which are an
//! editor wire protocol, not a codefang MACHINE-format report. The project's
//! byte-identity rule (route report serialization through `cf-gojson` /
//! `cf-goyaml`) therefore does not apply here: LSP framing is owned by
//! [`tower_lsp`] and is not part of the analyzer report surface. No raw report
//! bytes are produced by this crate.
//!
//! # Usage
//!
//! ```no_run
//! # async fn run() {
//! // Serve the mapping-DSL LSP over stdio (the `uast lsp` subcommand path).
//! cf_uast_lsp::run_stdio().await;
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod backend;
pub mod completion;
pub mod document_store;
pub mod text;

pub use backend::{Backend, SERVER_NAME, SERVER_VERSION};
pub use completion::{
    all_completions, completion_item, hover_doc, mapping_dsl_keywords, uast_fields,
};
pub use document_store::DocumentStore;
pub use text::{extract_word_at_position, is_word_char, split_lines};

use tower_lsp::{LspService, Server};

/// Runs the mapping-DSL LSP server on stdio until the client disconnects.
///
/// Port of Go `(*Server).Run`, which calls `server.NewServer(...).RunStdio()`.
/// Builds a [`tower_lsp::LspService`] around a fresh [`Backend`] and drives it
/// with stdin/stdout. Returns when the input stream closes (i.e. on shutdown).
pub async fn run_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

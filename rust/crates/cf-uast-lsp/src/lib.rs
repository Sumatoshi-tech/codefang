//! `cf-uast-lsp` — Language Server Protocol (LSP) server for the UAST mapping DSL.
//!
//! Used by the `uast lsp` subcommand. It provides editor support — completion,
//! hover, and document tracking — for the `.uastmap` mapping DSL used by the
//! UAST framework. Built on [`tower_lsp`] (DESIGN §3).
//!
//! # Modules
//!
//! * [`document_store`] — the thread-safe URI→content store.
//! * [`text`] — cursor/word helpers with byte-offset semantics.
//! * [`completion`] — the static keyword/field completion items and hover docs.
//! * [`backend`] — the [`tower_lsp::LanguageServer`] impl.
//!
//! # Serialization note (byte-identity)
//!
//! This server emits **only LSP protocol JSON-RPC messages**, which are an
//! editor wire protocol, not a codefang MACHINE-format report. The project's
//! byte-identity rule (route report serialization through `cf-gojson` /
//! `cf-goyaml`) therefore does not apply here: LSP framing is owned by
//! [`tower_lsp`] and is not part of the analyzer report surface. No raw
//! report bytes are produced by this crate.
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
/// Builds a [`tower_lsp::LspService`] around a fresh [`Backend`] and drives it
/// with stdin/stdout. Returns when the input stream closes (i.e. on
/// shutdown).
pub async fn run_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

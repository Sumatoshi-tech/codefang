//! tower-lsp [`LanguageServer`] implementation for the mapping-DSL server.
//!
//! Port of the Go `Server` type and its `protocol.Handler` wiring in
//! `pkg/uast/lsp/server.go`. The Go server uses the `tliron/glsp` stack; per
//! DESIGN §3 ("LSP server (tower-lsp implied) -> tower-lsp") the Rust port uses
//! [`tower_lsp`]. Handler behaviour is mirrored one-to-one:
//!
//! | Go `protocol.Handler` field   | tower-lsp trait method |
//! |-------------------------------|------------------------|
//! | `Initialize`                  | [`Backend::initialize`] |
//! | `Initialized`                 | [`Backend::initialized`] |
//! | `Shutdown`                    | [`Backend::shutdown`] |
//! | `TextDocumentDidOpen`         | [`Backend::did_open`] |
//! | `TextDocumentDidChange`       | [`Backend::did_change`] |
//! | `TextDocumentDidSave`         | [`Backend::did_save`] |
//! | `TextDocumentDidClose`        | [`Backend::did_close`] |
//! | `TextDocumentCompletion`      | [`Backend::completion`] |
//! | `TextDocumentHover`           | [`Backend::hover`] |
//!
//! `SetTrace` is handled transparently by tower-lsp's built-in `$/setTrace`
//! support, so there is no explicit method to port; the Go handler only toggled
//! a global trace value.

use std::sync::Arc;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionList, CompletionOptions, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, Hover, HoverContents, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, InitializedParams, MarkupContent, MarkupKind, MessageType,
    PublishDiagnosticsParams, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};
use tower_lsp::{Client, LanguageServer};

use crate::completion::{all_completions, hover_doc};
use crate::document_store::DocumentStore;
use crate::text::extract_word_at_position;

/// Server display name, byte-identical to the Go `server.NewServer(..., "uast mapping DSL", ...)`
/// argument and the `ServerInfo.Name` returned from `initialize`.
pub const SERVER_NAME: &str = "uast mapping DSL";

/// Server version reported in `initialize`, matching Go's hard-coded `"0.1.0"`.
pub const SERVER_VERSION: &str = "0.1.0";

/// The mapping-DSL LSP backend.
///
/// Equivalent to Go `lsp.Server`: it owns a shared [`DocumentStore`] and a
/// [`Client`] handle used to push `textDocument/publishDiagnostics`
/// notifications (the Go server used `ctx.Notify`).
#[derive(Debug)]
pub struct Backend {
    /// JSON-RPC client handle for sending notifications back to the editor.
    client: Client,
    /// Open-document content store (URI -> text).
    store: Arc<DocumentStore>,
}

impl Backend {
    /// Creates a new [`Backend`] bound to the given tower-lsp [`Client`].
    ///
    /// Analogous to Go `NewServer()`, which constructs an empty document store
    /// and registers the handler set. The handler registration is expressed in
    /// Rust by the [`LanguageServer`] trait impl below rather than a struct of
    /// function pointers.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            store: Arc::new(DocumentStore::new()),
        }
    }

    /// Returns a handle to the shared document store (primarily for testing).
    #[must_use]
    pub fn store(&self) -> Arc<DocumentStore> {
        Arc::clone(&self.store)
    }

    /// Publishes an (always empty) diagnostics set for `uri`.
    ///
    /// Port of Go `publishDiagnostics`: the mapping-DSL server performs no
    /// validation yet, so it clears diagnostics by sending an empty list. This
    /// mirrors `ctx.Notify("textDocument/publishDiagnostics", ...)` with an
    /// empty `Diagnostics` slice.
    async fn publish_diagnostics(&self, uri: tower_lsp::lsp_types::Url) {
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics: Vec::new(),
            version: None,
        };
        self.client
            .send_notification::<tower_lsp::lsp_types::notification::PublishDiagnostics>(params)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    /// `initialize` — advertise capabilities and server info.
    ///
    /// Port of Go `initialize`. The Go server advertises capabilities derived
    /// from the registered handlers (text-document sync, completion, hover) and
    /// returns `ServerInfo{Name: "uast mapping DSL", Version: "0.1.0"}`. We
    /// declare the equivalent capabilities explicitly: full-text sync (the
    /// `didChange` handler replaces the whole document), a completion provider,
    /// and a hover provider.
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // didChange installs the full new text, so we request FULL sync.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions::default()),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: SERVER_NAME.to_string(),
                version: Some(SERVER_VERSION.to_string()),
            }),
        })
    }

    /// `initialized` — log readiness then return, matching Go `initialized`
    /// (which is a pure no-op returning nil).
    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, format!("{SERVER_NAME} initialized"))
            .await;
    }

    /// `shutdown` — no-op success, matching Go `shutdown`.
    ///
    /// The Go handler also turned trace off (`SetTraceValue(TraceValueOff)`);
    /// tower-lsp manages trace state internally, so there is nothing to toggle.
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// `textDocument/didOpen` — store the opened text and publish diagnostics.
    ///
    /// Port of Go `didOpen`.
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.store.set(uri.to_string(), text);
        self.publish_diagnostics(uri).await;
    }

    /// `textDocument/didChange` — replace stored text with the latest change.
    ///
    /// Port of Go `didChange`, which reads the first content change's `text`
    /// field. Because we advertise FULL sync, each change carries the entire new
    /// document in `text` (and `range` is `None`); we take the first change, as
    /// the Go server does.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next() {
            self.store.set(uri.to_string(), change.text);
            self.publish_diagnostics(uri).await;
        }
    }

    /// `textDocument/didSave` — re-publish diagnostics if the doc is tracked.
    ///
    /// Port of Go `didSave`.
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if self.store.contains(uri.as_str()) {
            self.publish_diagnostics(uri).await;
        }
    }

    /// `textDocument/didClose` — drop the document from the store.
    ///
    /// Port of Go `didClose`.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.store.delete(params.text_document.uri.as_str());
    }

    /// `textDocument/completion` — offer mapping-DSL keywords + UAST fields.
    ///
    /// Port of Go `completion`: returns a non-incomplete completion list of the
    /// concatenated keyword and field items, in that order.
    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items: all_completions(),
        })))
    }

    /// `textDocument/hover` — Markdown docs for the word under the cursor.
    ///
    /// Port of Go `hover`: looks up the document, extracts the word at the
    /// cursor position, and returns its Markdown doc if one exists; otherwise
    /// returns `None` (the LSP "no hover" response, matching Go's `nil, nil`).
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let pos = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let Some(text) = self.store.get(uri.as_str()) else {
            return Ok(None);
        };

        let word = extract_word_at_position(&text, pos.line as usize, pos.character as usize);

        Ok(hover_doc(&word).map(|doc| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: doc.to_string(),
            }),
            range: None,
        }))
    }
}

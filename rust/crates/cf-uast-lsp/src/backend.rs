//! tower-lsp [`LanguageServer`] implementation for the mapping-DSL server.
//!
//! Handlers: [`Backend::initialize`], [`Backend::initialized`],
//! [`Backend::shutdown`], [`Backend::did_open`], [`Backend::did_change`],
//! [`Backend::did_save`], [`Backend::did_close`], [`Backend::completion`],
//! and [`Backend::hover`]. `$/setTrace` is handled transparently by
//! tower-lsp's built-in support.

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

/// Server display name, returned as `ServerInfo.name` from `initialize`
/// (frozen string).
pub const SERVER_NAME: &str = "uast mapping DSL";

/// Server version reported in `initialize` (frozen string).
pub const SERVER_VERSION: &str = "0.1.0";

/// The mapping-DSL LSP backend.
///
/// Owns a shared [`DocumentStore`] and a [`Client`] handle used to push
/// `textDocument/publishDiagnostics` notifications.
#[derive(Debug)]
pub struct Backend {
    /// JSON-RPC client handle for sending notifications back to the editor.
    client: Client,
    /// Open-document content store (URI -> text).
    store: Arc<DocumentStore>,
}

impl Backend {
    /// Creates a new [`Backend`] bound to the given tower-lsp [`Client`],
    /// with an empty document store.
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
    /// The mapping-DSL server performs no validation yet, so it clears
    /// diagnostics by sending an empty list.
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
    /// Capabilities: full-text sync (the `didChange` handler replaces the
    /// whole document), a completion provider, and a hover provider. The
    /// server info carries [`SERVER_NAME`] / [`SERVER_VERSION`].
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

    /// `initialized` — log readiness then return.
    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, format!("{SERVER_NAME} initialized"))
            .await;
    }

    /// `shutdown` — no-op success (tower-lsp manages trace state
    /// internally, so there is nothing to toggle).
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// `textDocument/didOpen` — store the opened text and publish diagnostics.
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.store.set(uri.to_string(), text);
        self.publish_diagnostics(uri).await;
    }

    /// `textDocument/didChange` — replace stored text with the latest change.
    ///
    /// Because the server advertises FULL sync, each change carries the entire
    /// new document in `text` (and `range` is `None`); the first change wins.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next() {
            self.store.set(uri.to_string(), change.text);
            self.publish_diagnostics(uri).await;
        }
    }

    /// `textDocument/didSave` — re-publish diagnostics if the doc is tracked.
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if self.store.contains(uri.as_str()) {
            self.publish_diagnostics(uri).await;
        }
    }

    /// `textDocument/didClose` — drop the document from the store.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.store.delete(params.text_document.uri.as_str());
    }

    /// `textDocument/completion` — offer mapping-DSL keywords + UAST fields.
    ///
    /// Returns a non-incomplete completion list of the concatenated keyword
    /// and field items, in that order.
    async fn completion(&self, _: CompletionParams) -> Result<Option<CompletionResponse>> {
        Ok(Some(CompletionResponse::List(CompletionList {
            is_incomplete: false,
            items: all_completions(),
        })))
    }

    /// `textDocument/hover` — Markdown docs for the word under the cursor.
    ///
    /// Looks up the document, extracts the word at the cursor position, and
    /// returns its Markdown doc if one exists; otherwise returns `None` (the
    /// LSP "no hover" response).
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

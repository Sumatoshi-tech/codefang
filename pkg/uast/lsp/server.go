// Package lsp provides a Language Server Protocol (LSP) server for the
// mapping DSL used in the UAST framework.
package lsp

import (
	"log"
	"strings"
	"sync"

	"github.com/tliron/glsp"
	protocol "github.com/tliron/glsp/protocol_3_16"
	"github.com/tliron/glsp/server"
)

// DocumentStore is a thread-safe store for document contents keyed by URI.
type DocumentStore struct {
	documents map[string]string // URI -> content.
	mu        sync.RWMutex
}

// NewDocumentStore creates a new empty DocumentStore.
func NewDocumentStore() *DocumentStore {
	return &DocumentStore{
		documents: make(map[string]string),
	}
}

// Set stores document content for the given URI.
func (ds *DocumentStore) Set(uri, content string) {
	ds.mu.Lock()
	defer ds.mu.Unlock()

	ds.documents[uri] = content
}

// Get retrieves document content by URI.
func (ds *DocumentStore) Get(uri string) (string, bool) {
	ds.mu.RLock()
	defer ds.mu.RUnlock()

	content, ok := ds.documents[uri]

	return content, ok
}

// Delete removes document content by URI.
func (ds *DocumentStore) Delete(uri string) {
	ds.mu.Lock()
	defer ds.mu.Unlock()

	delete(ds.documents, uri)
}

// Server implements the mapping DSL LSP server.
type Server struct {
	store   *DocumentStore
	handler protocol.Handler
}

// NewServer creates a new mapping DSL LSP server with default handlers.
func NewServer() *Server {
	srv := &Server{store: NewDocumentStore()}

	srv.handler = protocol.Handler{
		Initialize:             srv.initialize,
		Initialized:            srv.initialized,
		Shutdown:               srv.shutdown,
		SetTrace:               srv.setTrace,
		TextDocumentDidOpen:    srv.didOpen,
		TextDocumentDidChange:  srv.didChange,
		TextDocumentDidSave:    srv.didSave,
		TextDocumentDidClose:   srv.didClose,
		TextDocumentCompletion: srv.completion,
		TextDocumentHover:      srv.hover,
	}

	return srv
}

// Run starts the LSP server on stdio.
func (srv *Server) Run() {
	lspServer := server.NewServer(&srv.handler, "uast mapping DSL", false)

	err := lspServer.RunStdio()
	if err != nil {
		log.Printf("LSP server error: %v", err)
	}
}

func (srv *Server) initialize(_ *glsp.Context, _ *protocol.InitializeParams) (any, error) {
	capabilities := srv.handler.CreateServerCapabilities()
	version := "0.1.0"

	return protocol.InitializeResult{
		Capabilities: capabilities,
		ServerInfo: &protocol.InitializeResultServerInfo{
			Name:    "uast mapping DSL",
			Version: &version,
		},
	}, nil
}

func (srv *Server) initialized(_ *glsp.Context, _ *protocol.InitializedParams) error {
	return nil
}

func (srv *Server) shutdown(_ *glsp.Context) error {
	protocol.SetTraceValue(protocol.TraceValueOff)

	return nil
}

func (srv *Server) setTrace(_ *glsp.Context, params *protocol.SetTraceParams) error {
	protocol.SetTraceValue(params.Value)

	return nil
}

func (srv *Server) didOpen(ctx *glsp.Context, params *protocol.DidOpenTextDocumentParams) error {
	uri := params.TextDocument.URI
	text := params.TextDocument.Text

	srv.store.Set(uri, text)
	srv.publishDiagnostics(ctx, uri)

	return nil
}

func (srv *Server) didChange(ctx *glsp.Context, params *protocol.DidChangeTextDocumentParams) error {
	uri := params.TextDocument.URI

	if len(params.ContentChanges) > 0 {
		if change, changeOK := params.ContentChanges[0].(map[string]any); changeOK {
			if text, textOK := change["text"].(string); textOK {
				srv.store.Set(uri, text)
				srv.publishDiagnostics(ctx, uri)
			}
		}
	}

	return nil
}

func (srv *Server) didSave(ctx *glsp.Context, params *protocol.DidSaveTextDocumentParams) error {
	uri := params.TextDocument.URI

	if _, ok := srv.store.Get(uri); ok {
		srv.publishDiagnostics(ctx, uri)
	}

	return nil
}

func (srv *Server) didClose(_ *glsp.Context, params *protocol.DidCloseTextDocumentParams) error {
	uri := params.TextDocument.URI
	srv.store.Delete(uri)

	return nil
}

var (
	mappingDSLKeywords = []protocol.CompletionItem{
		completionItem("<-", protocol.CompletionItemKindKeyword, "Pattern assignment"),
		completionItem("=>", protocol.CompletionItemKindKeyword, "UAST mapping assignment"),
		completionItem("uast", protocol.CompletionItemKindKeyword, "UAST specification block"),
	}

	uastFields = []protocol.CompletionItem{
		completionItem("type", protocol.CompletionItemKindField, "UAST node type (string)"),
		completionItem("token", protocol.CompletionItemKindField, "Token/capture for node label"),
		completionItem("roles", protocol.CompletionItemKindField, "UAST node roles (list)"),
		completionItem("props", protocol.CompletionItemKindField, "UAST node properties (map)"),
		completionItem("children", protocol.CompletionItemKindField,
			"UAST children (list of captures)"),
	}

	hoverDocs = map[string]string{
		"<-":       "Assigns a pattern to a rule name. Example: `rule <- (pattern)`.",
		"=>":       "Assigns a UAST mapping to a pattern. Example: `(pattern) => uast(...)`.",
		"uast":     "Begins a UAST specification block for mapping output.",
		"type":     "UAST node type. Example: `type: \"Function\"`.",
		"token":    "Token or capture used as the node label. Example: `token: \"@name\"`.",
		"roles":    "List of UAST roles for this node. Example: `roles: \"Declaration\"`.",
		"props":    "Map of additional node properties. Example: `props: [\"receiver\": \"true\"]`.",
		"children": "List of child captures for this node. Example: `children: [\"@body\"]`.",
	}
)

func completionItem(label string, kind protocol.CompletionItemKind, detail string) protocol.CompletionItem {
	return protocol.CompletionItem{
		Label:  label,
		Kind:   &kind,
		Detail: &detail,
	}
}

func (srv *Server) completion(_ *glsp.Context, _ *protocol.CompletionParams) (any, error) {
	// Suggest mapping DSL keywords and UAST fields.
	items := make([]protocol.CompletionItem, 0, len(mappingDSLKeywords)+len(uastFields))
	items = append(items, mappingDSLKeywords...)
	items = append(items, uastFields...)

	return protocol.CompletionList{IsIncomplete: false, Items: items}, nil
}

func (srv *Server) hover(_ *glsp.Context, params *protocol.HoverParams) (*protocol.Hover, error) {
	// Find the word under the cursor.
	uri := params.TextDocument.URI
	pos := params.Position

	text, ok := srv.store.Get(uri)
	if !ok {
		return nil, nil // LSP protocol expects nil hover when no document found.
	}

	word := extractWordAtPosition(text, int(pos.Line), int(pos.Character))

	if doc, found := hoverDocs[word]; found {
		return &protocol.Hover{
			Contents: protocol.MarkupContent{
				Kind:  protocol.MarkupKindMarkdown,
				Value: doc,
			},
		}, nil
	}

	return nil, nil // LSP protocol expects nil hover when no docs available.
}

// extractWordAtPosition returns the word at the given line/character in the text.
func extractWordAtPosition(text string, line, character int) string {
	lines := splitLines(text)
	if line >= len(lines) {
		return ""
	}

	lineText := lines[line]
	if character > len(lineText) {
		character = len(lineText)
	}

	start := character

	for start > 0 && isWordChar(lineText[start-1]) {
		start--
	}

	end := character

	for end < len(lineText) && isWordChar(lineText[end]) {
		end++
	}

	return lineText[start:end]
}

func isWordChar(ch byte) bool {
	return (ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') ||
		ch == '_' || ch == '<' || ch == '>' || ch == '-' || ch == '='
}

func splitLines(input string) []string {
	return strings.Split(input, "\n")
}

func (srv *Server) publishDiagnostics(ctx *glsp.Context, uri string) {
	ctx.Notify("textDocument/publishDiagnostics", &protocol.PublishDiagnosticsParams{
		URI:         uri,
		Diagnostics: []protocol.Diagnostic{},
	})
}

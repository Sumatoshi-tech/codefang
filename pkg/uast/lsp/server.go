package lsp

import (
	"log"
	"strings"
	"sync"

	"github.com/tliron/glsp"
	protocol "github.com/tliron/glsp/protocol_3_16"
	"github.com/tliron/glsp/server"
)

type DocumentStore struct {
	mu        sync.RWMutex
	documents map[string]string // URI -> content
}

func NewDocumentStore() *DocumentStore {
	return &DocumentStore{documents: make(map[string]string)}
}

func (ds *DocumentStore) Set(uri, content string) {
	ds.mu.Lock()
	defer ds.mu.Unlock()
	ds.documents[uri] = content
}

func (ds *DocumentStore) Get(uri string) (string, bool) {
	ds.mu.RLock()
	defer ds.mu.RUnlock()
	content, ok := ds.documents[uri]
	return content, ok
}

func (ds *DocumentStore) Delete(uri string) {
	ds.mu.Lock()
	defer ds.mu.Unlock()
	delete(ds.documents, uri)
}

// Server implements the mapping DSL LSP server
type Server struct {
	store   *DocumentStore
	handler protocol.Handler
}

func NewServer() *Server {
	s := &Server{store: NewDocumentStore()}
	s.handler = protocol.Handler{
		Initialize:             s.initialize,
		Initialized:            s.initialized,
		Shutdown:               s.shutdown,
		SetTrace:               s.setTrace,
		TextDocumentDidOpen:    s.didOpen,
		TextDocumentDidChange:  s.didChange,
		TextDocumentDidSave:    s.didSave,
		TextDocumentDidClose:   s.didClose,
		TextDocumentCompletion: s.completion,
		TextDocumentHover:      s.hover,
	}
	return s
}

func (s *Server) Run() {
	srv := server.NewServer(&s.handler, "uast mapping DSL", false)
	if err := srv.RunStdio(); err != nil {
		log.Printf("LSP server error: %v", err)
	}
}

func (s *Server) initialize(ctx *glsp.Context, params *protocol.InitializeParams) (any, error) {
	capabilities := s.handler.CreateServerCapabilities()
	version := "0.1.0"
	return protocol.InitializeResult{
		Capabilities: capabilities,
		ServerInfo: &protocol.InitializeResultServerInfo{
			Name:    "uast mapping DSL",
			Version: &version,
		},
	}, nil
}

func (s *Server) initialized(ctx *glsp.Context, params *protocol.InitializedParams) error {
	return nil
}

func (s *Server) shutdown(ctx *glsp.Context) error {
	protocol.SetTraceValue(protocol.TraceValueOff)
	return nil
}

func (s *Server) setTrace(ctx *glsp.Context, params *protocol.SetTraceParams) error {
	protocol.SetTraceValue(params.Value)
	return nil
}

func (s *Server) didOpen(ctx *glsp.Context, params *protocol.DidOpenTextDocumentParams) error {
	uri := params.TextDocument.URI
	text := params.TextDocument.Text
	s.store.Set(uri, text)
	s.publishDiagnostics(ctx, uri, text)
	return nil
}

func (s *Server) didChange(ctx *glsp.Context, params *protocol.DidChangeTextDocumentParams) error {
	uri := params.TextDocument.URI
	if len(params.ContentChanges) > 0 {
		if change, ok := params.ContentChanges[0].(map[string]interface{}); ok {
			if text, ok := change["text"].(string); ok {
				s.store.Set(uri, text)
				s.publishDiagnostics(ctx, uri, text)
			}
		}
	}
	return nil
}

func (s *Server) didSave(ctx *glsp.Context, params *protocol.DidSaveTextDocumentParams) error {
	uri := params.TextDocument.URI
	if text, ok := s.store.Get(uri); ok {
		s.publishDiagnostics(ctx, uri, text)
	}
	return nil
}

func (s *Server) didClose(ctx *glsp.Context, params *protocol.DidCloseTextDocumentParams) error {
	uri := params.TextDocument.URI
	s.store.Delete(uri)
	return nil
}

func ptrCompletionKind(k protocol.CompletionItemKind) *protocol.CompletionItemKind { return &k }
func ptrString(s string) *string                                                   { return &s }

var (
	mappingDSLKeywords = []protocol.CompletionItem{
		{Label: "<-", Kind: ptrCompletionKind(protocol.CompletionItemKindKeyword), Detail: ptrString("Pattern assignment")},
		{Label: "=>", Kind: ptrCompletionKind(protocol.CompletionItemKindKeyword), Detail: ptrString("UAST mapping assignment")},
		{Label: "uast", Kind: ptrCompletionKind(protocol.CompletionItemKindKeyword), Detail: ptrString("UAST specification block")},
	}

	uastFields = []protocol.CompletionItem{
		{Label: "type", Kind: ptrCompletionKind(protocol.CompletionItemKindField), Detail: ptrString("UAST node type (string)")},
		{Label: "token", Kind: ptrCompletionKind(protocol.CompletionItemKindField), Detail: ptrString("Token/capture for node label")},
		{Label: "roles", Kind: ptrCompletionKind(protocol.CompletionItemKindField), Detail: ptrString("UAST node roles (list)")},
		{Label: "props", Kind: ptrCompletionKind(protocol.CompletionItemKindField), Detail: ptrString("UAST node properties (map)")},
		{Label: "children", Kind: ptrCompletionKind(protocol.CompletionItemKindField), Detail: ptrString("UAST children (list of captures)")},
	}

	hoverDocs = map[string]string{
		"<-":       "Assigns a pattern to a rule name. Example: `rule <- (pattern)`.",
		"=>":       "Assigns a UAST mapping to a pattern. Example: `(pattern) => uast(...)`.",
		"uast":     "Begins a UAST specification block for mapping output.",
		"type":     "UAST node type. Example: `type: \"Function\"`.",
		"token":    "Token or capture used as the node label. Example: `token: \"@name\"`.",
		"roles":    "List of UAST roles for this node. Example: `roles: \"Declaration\"`. ",
		"props":    "Map of additional node properties. Example: `props: [\"receiver\": \"true\"]`.",
		"children": "List of child captures for this node. Example: `children: [\"@body\"]`.",
	}
)

func (s *Server) completion(ctx *glsp.Context, params *protocol.CompletionParams) (any, error) {
	// For now, always suggest mapping DSL keywords and UAST fields
	items := make([]protocol.CompletionItem, 0, len(mappingDSLKeywords)+len(uastFields))
	items = append(items, mappingDSLKeywords...)
	items = append(items, uastFields...)
	return protocol.CompletionList{IsIncomplete: false, Items: items}, nil
}

func (s *Server) hover(ctx *glsp.Context, params *protocol.HoverParams) (*protocol.Hover, error) {
	// Find the word under the cursor
	uri := params.TextDocument.URI
	pos := params.Position
	text, ok := s.store.Get(uri)
	if !ok {
		return nil, nil
	}
	word := extractWordAtPosition(text, int(pos.Line), int(pos.Character))
	if doc, ok := hoverDocs[word]; ok {
		return &protocol.Hover{
			Contents: protocol.MarkupContent{
				Kind:  protocol.MarkupKindMarkdown,
				Value: doc,
			},
		}, nil
	}
	return nil, nil
}

// extractWordAtPosition returns the word at the given line/character in the text
func extractWordAtPosition(text string, line, character int) string {
	lines := splitLines(text)
	if line >= len(lines) {
		return ""
	}
	l := lines[line]
	if character > len(l) {
		character = len(l)
	}
	start := character
	for start > 0 && isWordChar(l[start-1]) {
		start--
	}
	end := character
	for end < len(l) && isWordChar(l[end]) {
		end++
	}
	return l[start:end]
}

func isWordChar(c byte) bool {
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_' || c == '<' || c == '>' || c == '-' || c == '='
}

func splitLines(s string) []string {
	return strings.Split(s, "\n")
}

func (s *Server) publishDiagnostics(ctx *glsp.Context, uri string, text string) {
	ctx.Notify("textDocument/publishDiagnostics", &protocol.PublishDiagnosticsParams{
		URI:         uri,
		Diagnostics: []protocol.Diagnostic{},
	})
}

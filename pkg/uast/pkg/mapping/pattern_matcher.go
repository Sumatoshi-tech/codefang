package mapping

import (
	"fmt"
	"sync"

	sitter "github.com/alexaandru/go-tree-sitter-bare"
)

// PatternMatcher compiles and matches S-expression patterns to Tree-sitter queries.
type PatternMatcher struct {
	cache map[string]*sitter.Query
	mu    sync.RWMutex
	lang  *sitter.Language
}

// NewPatternMatcher creates a new PatternMatcher with an empty cache and language.
func NewPatternMatcher(lang *sitter.Language) *PatternMatcher {
	return &PatternMatcher{cache: make(map[string]*sitter.Query), lang: lang}
}

// CompileAndCache compiles a pattern and caches the result.
func (pm *PatternMatcher) CompileAndCache(pattern string) (*sitter.Query, error) {
	pm.mu.RLock()
	if q, ok := pm.cache[pattern]; ok {
		pm.mu.RUnlock()
		return q, nil
	}
	pm.mu.RUnlock()
	q, err := compileTreeSitterQuery(pattern, pm.lang)
	if err != nil {
		return nil, err
	}
	pm.mu.Lock()
	pm.cache[pattern] = q
	pm.mu.Unlock()
	return q, nil
}

// MatchPattern matches a compiled query against a Tree-sitter node and returns captures.
func (pm *PatternMatcher) MatchPattern(query *sitter.Query, node *sitter.Node, source []byte) (map[string]string, error) {
	return matchTreeSitterQuery(query, node, source)
}

// compileTreeSitterQuery compiles a pattern to a Tree-sitter query object.
func compileTreeSitterQuery(pattern string, lang *sitter.Language) (*sitter.Query, error) {
	if lang == nil {
		return nil, fmt.Errorf("tree-sitter language is nil")
	}
	q, err := sitter.NewQuery(lang, []byte(pattern))
	if err != nil {
		return nil, fmt.Errorf("tree-sitter query compilation failed: %w", err)
	}
	return q, nil
}

// matchTreeSitterQuery matches a query against a node and returns the first set of captures as a map.
func matchTreeSitterQuery(query *sitter.Query, node *sitter.Node, source []byte) (map[string]string, error) {
	if query == nil || node == nil {
		return nil, fmt.Errorf("query or node is nil")
	}
	cursor := sitter.NewQueryCursor()
	// Use Matches with dereferenced node
	matches := cursor.Matches(query, *node, source)
	match := matches.Next()
	if match == nil {
		return nil, fmt.Errorf("no match found")
	}
	captures := make(map[string]string)
	for _, cap := range match.Captures {
		name := query.CaptureNameForID(cap.Index)
		if !cap.Node.IsNull() {
			captures[name] = cap.Node.Content(source)
		}
	}
	return captures, nil
}

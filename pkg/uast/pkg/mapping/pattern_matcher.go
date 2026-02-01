package mapping

import (
	"errors"
	"fmt"
	"sync"

	sitter "github.com/alexaandru/go-tree-sitter-bare"
)

// Sentinel errors for pattern matching.
var (
	errNilLanguage = errors.New("tree-sitter language is nil")
	errNilQueryArg = errors.New("query or node is nil")
	errNoMatch     = errors.New("no match found")
)

// PatternMatcher compiles and matches S-expression patterns to Tree-sitter queries.
type PatternMatcher struct {
	cache  map[string]*sitter.Query
	lang   *sitter.Language
	mu     sync.RWMutex
	hits   int64
	misses int64
}

// NewPatternMatcher creates a new PatternMatcher with an empty cache and language.
func NewPatternMatcher(lang *sitter.Language) *PatternMatcher {
	return &PatternMatcher{
		cache: make(map[string]*sitter.Query),
		lang:  lang,
	}
}

// CompileAndCache compiles a pattern and caches the result.
func (pm *PatternMatcher) CompileAndCache(pattern string) (*sitter.Query, error) {
	pm.mu.RLock()

	if cachedQuery, ok := pm.cache[pattern]; ok {
		pm.hits++
		pm.mu.RUnlock()

		return cachedQuery, nil
	}

	pm.mu.RUnlock()

	compiled, err := compileTreeSitterQuery(pattern, pm.lang)
	if err != nil {
		return nil, err
	}

	pm.mu.Lock()
	pm.cache[pattern] = compiled
	pm.misses++
	pm.mu.Unlock()

	return compiled, nil
}

// CacheStats returns the number of cache hits and misses.
func (pm *PatternMatcher) CacheStats() (hits, misses int64) {
	pm.mu.RLock()
	defer pm.mu.RUnlock()

	return pm.hits, pm.misses
}

// MatchPattern matches a compiled query against a Tree-sitter node and returns captures.
func (pm *PatternMatcher) MatchPattern(query *sitter.Query, tsNode *sitter.Node, source []byte) (map[string]string, error) {
	return matchTreeSitterQuery(query, tsNode, source)
}

// compileTreeSitterQuery compiles a pattern to a Tree-sitter query object.
func compileTreeSitterQuery(pattern string, lang *sitter.Language) (*sitter.Query, error) {
	if lang == nil {
		return nil, errNilLanguage
	}

	compiled, err := sitter.NewQuery(lang, []byte(pattern))
	if err != nil {
		return nil, fmt.Errorf("tree-sitter query compilation failed: %w", err)
	}

	return compiled, nil
}

// matchTreeSitterQuery matches a query against a node and returns the first set of captures as a map.
func matchTreeSitterQuery(query *sitter.Query, tsNode *sitter.Node, source []byte) (map[string]string, error) {
	if query == nil || tsNode == nil {
		return nil, errNilQueryArg
	}

	cursor := sitter.NewQueryCursor()

	// Use Matches with dereferenced node.
	matches := cursor.Matches(query, *tsNode, source)

	match := matches.Next()
	if match == nil {
		return nil, errNoMatch
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

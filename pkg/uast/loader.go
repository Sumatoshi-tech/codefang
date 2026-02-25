package uast

import (
	"context"
	"fmt"
	"io"
	"io/fs"
	"log/slog"
	"strings"
	"sync"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/mapping"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// PrecompiledMapping represents the pre-compiled mapping data
type PrecompiledMapping struct {
	Language   string                 `json:"language"`
	Extensions []string               `json:"extensions"`
	Rules      []mapping.Rule         `json:"rules"`
	Patterns   map[string]interface{} `json:"patterns"`
	CompiledAt string                 `json:"compiled_at"`
}

// bloomSize is the bit-array length for the extension bloom filter.
// With ~200 registered extensions and 2 hash functions, 512 bits
// gives a false-positive rate under 5%.
const bloomSize = 512

// Loader loads UAST parsers for different languages.
type Loader struct {
	embedFS    fs.FS
	parsers    map[string]LanguageParser
	extensions map[string]LanguageParser
	extBloom   [bloomSize / 64]uint64
}

// NewLoader creates a new loader with the given embedded filesystem.
func NewLoader(embedFS fs.FS) *Loader {
	l := &Loader{
		embedFS:    embedFS,
		parsers:    make(map[string]LanguageParser),
		extensions: make(map[string]LanguageParser),
	}

	l.loadUASTParsers()

	return l
}

func (l *Loader) loadUASTParsers() {
	if l.loadFromCache() {
		return
	}

	l.loadFromFiles()
}

func (l *Loader) loadFromCache() bool {
	return l.loadFromEmbeddedMappings()
}

func (l *Loader) loadFromEmbeddedMappings() bool {
	if embeddedMappingsAvailable() {
		return l.loadFromEmbeddedMappingsLazy()
	}

	return false
}

// loadFromEmbeddedMappingsLazy registers lazy-initialized parsers.
// Tree-sitter language initialization is deferred until first parse call,
// avoiding the O(N) startup cost of initializing all 60+ languages.
// Extensions are added to a bloom filter for fast negative lookups during
// directory walking.
func (l *Loader) loadFromEmbeddedMappingsLazy() bool {
	for _, pm := range embeddedMappingsData {
		lazy := newLazyDSLParser(pm)
		l.parsers[pm.Language] = lazy

		for _, ext := range pm.Extensions {
			lower := strings.ToLower(ext)
			l.extensions[lower] = lazy
			l.bloomAdd(lower)
		}
	}

	return len(embeddedMappingsData) > 0
}

func (l *Loader) loadFromFiles() {
	if l.embedFS == nil {
		return
	}

	err := fs.WalkDir(l.embedFS, "uastmaps", func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if d.IsDir() {
			return nil
		}

		if !strings.HasSuffix(path, ".uastmap") {
			return nil
		}

		file, err := l.embedFS.Open(path)
		if err != nil {
			return err
		}
		defer file.Close()

		p, err := l.LoadParser(file)
		if err != nil {
			slog.Default().Warn("failed to load parser", "file", d.Name(), "error", err)
			return nil
		}

		l.parsers[p.Language()] = p

		for _, ext := range p.Extensions() {
			lower := strings.ToLower(ext)
			l.extensions[lower] = p
			l.bloomAdd(lower)
		}

		return nil
	})

	if err != nil {
		slog.Default().Error("error discovering parsers", "error", err)
	}
}

// LoadParser loads a parser by reading the uastmap file through the reader
func (l *Loader) LoadParser(reader io.Reader) (LanguageParser, error) {
	dslp := NewDSLParser(reader)

	var err error
	func() {
		defer func() {
			if r := recover(); r != nil {
				err = fmt.Errorf("panic while loading parser: %v", r)
			}
		}()
		err = dslp.Load()
	}()

	if err != nil {
		return nil, err
	}

	return dslp, nil
}

// LanguageParser returns the parser for the given file extension.
// A bloom filter provides a fast negative check: if the extension
// is definitely not registered, the map lookup is skipped entirely.
func (l *Loader) LanguageParser(extension string) (LanguageParser, bool) {
	ext := strings.ToLower(extension)
	if !l.bloomMayContain(ext) {
		return nil, false
	}

	parser, exists := l.extensions[ext]

	return parser, exists
}

func (l *Loader) bloomAdd(ext string) {
	h1, h2 := bloomHashes(ext)
	l.extBloom[h1/64] |= 1 << (h1 % 64)
	l.extBloom[h2/64] |= 1 << (h2 % 64)
}

func (l *Loader) bloomMayContain(ext string) bool {
	h1, h2 := bloomHashes(ext)

	return l.extBloom[h1/64]&(1<<(h1%64)) != 0 &&
		l.extBloom[h2/64]&(1<<(h2%64)) != 0
}

// bloomHashes returns two independent bit positions for a bloom filter.
// Uses FNV-1a variant with two different seeds for the two hash functions.
func bloomHashes(s string) (uint, uint) {
	const (
		fnvBasis1 uint = 14695981039346656037
		fnvBasis2 uint = 17316225907498340287
		fnvPrime  uint = 1099511628211
	)

	h1, h2 := fnvBasis1, fnvBasis2

	for i := range len(s) {
		h1 ^= uint(s[i])
		h1 *= fnvPrime
		h2 ^= uint(s[i])
		h2 *= fnvPrime
	}

	return h1 % bloomSize, h2 % bloomSize
}

// GetParsers returns all loaded parsers.
func (l *Loader) GetParsers() map[string]LanguageParser {
	return l.parsers
}

// lazyDSLParser wraps a PrecompiledMapping and defers tree-sitter
// language initialization until the first Parse() call. This avoids
// loading pattern matchers and symbol tables for languages that are
// never used in the current run.
type lazyDSLParser struct {
	mapping    PrecompiledMapping
	once       sync.Once
	parser     *DSLParser
	initErr    error
	extensions []string
	language   string
}

func newLazyDSLParser(pm PrecompiledMapping) *lazyDSLParser {
	return &lazyDSLParser{
		mapping:    pm,
		extensions: pm.Extensions,
		language:   pm.Language,
	}
}

func (lp *lazyDSLParser) init() {
	lp.once.Do(func() {
		lp.parser = &DSLParser{
			langInfo: &mapping.LanguageInfo{
				Name:       lp.mapping.Language,
				Extensions: lp.mapping.Extensions,
			},
			mappingRules: lp.mapping.Rules,
		}

		lp.initErr = lp.parser.initializeLanguage()
	})
}

func (lp *lazyDSLParser) Parse(ctx context.Context, filename string, content []byte) (*node.Node, error) {
	lp.init()
	if lp.initErr != nil {
		return nil, lp.initErr
	}

	return lp.parser.Parse(ctx, filename, content)
}

func (lp *lazyDSLParser) Language() string {
	return lp.language
}

func (lp *lazyDSLParser) Extensions() []string {
	return lp.extensions
}

func (lp *lazyDSLParser) GetOriginalDSL() string {
	lp.init()
	if lp.parser == nil {
		return ""
	}

	return lp.parser.GetOriginalDSL()
}

package uast

import (
	"context"
	"embed"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"maps"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

//go:embed uastmaps/*.uastmap
var uastMapFs embed.FS

// Sentinel errors for parser operations.
var (
	errNoFileExtension = errors.New("no file extension found")
	errNoParser        = errors.New("no parser found for extension")
	errMappingNotFound = errors.New("mapping not found")
)

// Parser implements LanguageParser using embedded parsers.
// Entry point for UAST parsing.
// Parser is the main entry point for UAST parsing. It manages language parsers and their configurations.
type Parser struct {
	loader     *Loader
	customMaps map[string]Map
}

// NewParser creates a new Parser with DSL-based language parsers.
// It loads parser configurations and instantiates parsers for each supported language.
// Returns a pointer to the Parser or an error if loading parsers fails.
func NewParser() (*Parser, error) {
	loader := NewLoader(uastMapFs)

	parser := &Parser{
		loader:     loader,
		customMaps: make(map[string]Map),
	}

	return parser, nil
}

// WithMap adds custom UAST mappings to the parser using the option pattern.
// This method allows passing custom UAST map configurations that will be used
// in addition to or as a replacement for the embedded mappings.
func (parser *Parser) WithMap(uastMaps map[string]Map) *Parser {
	// Store custom maps.
	maps.Copy(parser.customMaps, uastMaps)

	// Load custom parsers from the provided mappings.
	parser.loadCustomParsers()

	return parser
}

// loadCustomParsers loads parsers from custom UAST mappings.
func (parser *Parser) loadCustomParsers() {
	for _, uastMap := range parser.customMaps {
		// Create a reader from the UAST string.
		reader := strings.NewReader(uastMap.UAST)

		// Load parser from the custom mapping.
		langParser, loadErr := parser.loader.LoadParser(reader)
		if loadErr != nil {
			continue
		}

		// Register the parser with the loader.
		parser.loader.parsers[langParser.Language()] = langParser

		// Register extensions.
		for _, ext := range langParser.Extensions() {
			parser.loader.extensions[strings.ToLower(ext)] = langParser
		}
	}
}

// IsSupported returns true if the given filename is supported by any parser.
func (parser *Parser) IsSupported(filename string) bool {
	// Get file extension.
	ext := strings.ToLower(getFileExtension(filename))
	if ext == "" {
		return false
	}

	// Check if any parser supports this file extension.
	_, exists := parser.loader.LanguageParser(ext)

	return exists
}

// GetLanguage returns the language name for the given filename if supported, or empty string.
func (parser *Parser) GetLanguage(filename string) string {
	ext := strings.ToLower(getFileExtension(filename))
	if ext == "" {
		return ""
	}

	langParser, exists := parser.loader.LanguageParser(ext)
	if !exists {
		return ""
	}

	return langParser.Language()
}

// Parse parses a file and returns its UAST.
func (parser *Parser) Parse(ctx context.Context, filename string, content []byte) (*node.Node, error) {
	ext := strings.ToLower(getFileExtension(filename))
	if ext == "" {
		return nil, fmt.Errorf("%w for %s", errNoFileExtension, filename)
	}

	langParser, exists := parser.loader.LanguageParser(ext)
	if !exists {
		return nil, fmt.Errorf("%w %s", errNoParser, ext)
	}

	return langParser.Parse(ctx, filename, content)
}

// GetEmbeddedMappings returns all embedded UAST mappings.
func (parser *Parser) GetEmbeddedMappings() map[string]Map {
	mappings := make(map[string]Map)

	// Read from the embedded filesystem.
	walkErr := fs.WalkDir(uastMapFs, "uastmaps", func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if entry.IsDir() {
			return nil
		}

		if !strings.HasSuffix(path, ".uastmap") {
			return nil
		}

		// Extract language name from filename.
		language := strings.TrimSuffix(entry.Name(), ".uastmap")

		// Read the file content.
		file, openErr := uastMapFs.Open(path)
		if openErr != nil {
			return nil
		}
		defer file.Close()

		content, readErr := io.ReadAll(file)
		if readErr != nil {
			return nil
		}

		// Parse the DSL to get extensions.
		dslParser := NewDSLParser(strings.NewReader(string(content)))

		parseErr := dslParser.Load()
		if parseErr != nil {
			return nil
		}

		mappings[language] = Map{
			UAST:       string(content),
			Extensions: dslParser.Extensions(),
		}

		return nil
	})
	_ = walkErr

	return mappings
}

// GetEmbeddedMappingsList returns a lightweight list of embedded UAST mappings (without full content).
func (parser *Parser) GetEmbeddedMappingsList() map[string]map[string]any {
	mappings := make(map[string]map[string]any)

	// Read from the embedded filesystem.
	walkErr := fs.WalkDir(uastMapFs, "uastmaps", func(path string, entry fs.DirEntry, err error) error {
		if err != nil {
			return err
		}

		if entry.IsDir() {
			return nil
		}

		if !strings.HasSuffix(path, ".uastmap") {
			return nil
		}

		// Extract language name from filename.
		language := strings.TrimSuffix(entry.Name(), ".uastmap")

		// Read the file content.
		file, openErr := uastMapFs.Open(path)
		if openErr != nil {
			return nil
		}
		defer file.Close()

		content, readErr := io.ReadAll(file)
		if readErr != nil {
			return nil
		}

		// For the list endpoint, provide basic file info.
		mappings[language] = map[string]any{
			"size": len(content),
		}

		return nil
	})
	_ = walkErr

	return mappings
}

// GetMapping returns a specific embedded UAST mapping by name.
func (parser *Parser) GetMapping(language string) (*Map, error) {
	// Construct the file path.
	filePath := fmt.Sprintf("uastmaps/%s.uastmap", language)

	// Read the file content.
	file, err := uastMapFs.Open(filePath)
	if err != nil {
		return nil, fmt.Errorf("%w: %s", errMappingNotFound, language)
	}
	defer file.Close()

	content, err := io.ReadAll(file)
	if err != nil {
		return nil, fmt.Errorf("reading mapping: %w", err)
	}

	// Parse the DSL to get extensions.
	dslParser := NewDSLParser(strings.NewReader(string(content)))

	parseErr := dslParser.Load()
	if parseErr != nil {
		return nil, fmt.Errorf("parsing DSL: %w", parseErr)
	}

	return &Map{
		UAST:       string(content),
		Extensions: dslParser.Extensions(),
	}, nil
}

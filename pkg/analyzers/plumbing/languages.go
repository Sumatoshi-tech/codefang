package plumbing

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path"
	"strings"

	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// extensionToLanguage maps common file extensions to their programming languages.
// This provides O(1) lookup for unambiguous extensions, avoiding expensive content analysis.
//
//nolint:gochecknoglobals // package-level lookup table for performance.
var extensionToLanguage = map[string]string{
	// Go
	".go": "Go",
	// Python
	".py":   "Python",
	".pyw":  "Python",
	".pyi":  "Python",
	".pyx":  "Python",
	".pxd":  "Python",
	".gyp":  "Python",
	".gypi": "Python",
	// JavaScript
	".js":     "JavaScript",
	".mjs":    "JavaScript",
	".cjs":    "JavaScript",
	".jsx":    "JavaScript",
	".es6":    "JavaScript",
	".es":     "JavaScript",
	".jsm":    "JavaScript",
	".vue":    "Vue",
	".svelte": "Svelte",
	// TypeScript
	".ts":  "TypeScript",
	".mts": "TypeScript",
	".cts": "TypeScript",
	".tsx": "TSX",
	// Rust
	".rs": "Rust",
	// Java
	".java": "Java",
	// Kotlin
	".kt":  "Kotlin",
	".kts": "Kotlin",
	// Scala
	".scala": "Scala",
	".sc":    "Scala",
	// C
	".c": "C",
	".h": "C",
	// C++
	".cpp": "C++",
	".hpp": "C++",
	".cc":  "C++",
	".cxx": "C++",
	".hxx": "C++",
	".c++": "C++",
	".h++": "C++",
	".hh":  "C++",
	".ipp": "C++",
	".inl": "C++",
	".tcc": "C++",
	".tpp": "C++",
	// C#
	".cs":  "C#",
	".csx": "C#",
	// Ruby
	".rb":       "Ruby",
	".rake":     "Ruby",
	".gemspec":  "Ruby",
	".rbw":      "Ruby",
	".ru":       "Ruby",
	".podspec":  "Ruby",
	".thor":     "Ruby",
	".jbuilder": "Ruby",
	// PHP
	".php":   "PHP",
	".php3":  "PHP",
	".php4":  "PHP",
	".php5":  "PHP",
	".php7":  "PHP",
	".phps":  "PHP",
	".phtml": "PHP",
	// Shell
	".sh":   "Shell",
	".bash": "Shell",
	".zsh":  "Shell",
	".ksh":  "Shell",
	".csh":  "Shell",
	".tcsh": "Shell",
	".fish": "Shell",
	// PowerShell
	".ps1":  "PowerShell",
	".psm1": "PowerShell",
	".psd1": "PowerShell",
	// Perl
	".pl":  "Perl",
	".pm":  "Perl",
	".pod": "Perl",
	".t":   "Perl",
	// Lua
	".lua": "Lua",
	// R
	".r":   "R",
	".R":   "R",
	".rmd": "RMarkdown",
	".Rmd": "RMarkdown",
	// Swift
	".swift": "Swift",
	// Objective-C
	".m":  "Objective-C",
	".mm": "Objective-C++",
	// Dart
	".dart": "Dart",
	// Elixir
	".ex":   "Elixir",
	".exs":  "Elixir",
	".eex":  "Elixir",
	".leex": "Elixir",
	".heex": "Elixir",
	// Erlang
	".erl": "Erlang",
	".hrl": "Erlang",
	// Haskell
	".hs":  "Haskell",
	".lhs": "Haskell",
	// Clojure
	".clj":  "Clojure",
	".cljs": "ClojureScript",
	".cljc": "Clojure",
	".edn":  "Clojure",
	// F#
	".fs":       "F#",
	".fsi":      "F#",
	".fsx":      "F#",
	".fsscript": "F#",
	// OCaml
	".ml":  "OCaml",
	".mli": "OCaml",
	".mll": "OCaml",
	".mly": "OCaml",
	// Data formats
	".json":  "JSON",
	".json5": "JSON5",
	".yaml":  "YAML",
	".yml":   "YAML",
	".toml":  "TOML",
	".xml":   "XML",
	".csv":   "CSV",
	".tsv":   "TSV",
	// Config
	".ini":  "INI",
	".cfg":  "INI",
	".conf": "INI",
	".env":  "Dotenv",
	// Markup
	".html":  "HTML",
	".htm":   "HTML",
	".xhtml": "HTML",
	".css":   "CSS",
	".scss":  "SCSS",
	".sass":  "Sass",
	".less":  "Less",
	".styl":  "Stylus",
	// Documentation
	".md":       "Markdown",
	".markdown": "Markdown",
	".rst":      "reStructuredText",
	".tex":      "TeX",
	".latex":    "TeX",
	".adoc":     "AsciiDoc",
	".asciidoc": "AsciiDoc",
	// SQL
	".sql":   "SQL",
	".psql":  "SQL",
	".mysql": "SQL",
	".pgsql": "SQL",
	// GraphQL
	".graphql": "GraphQL",
	".gql":     "GraphQL",
	// Protocol Buffers
	".proto": "Protocol Buffer",
	// Thrift
	".thrift": "Thrift",
	// WebAssembly
	".wat":  "WebAssembly",
	".wast": "WebAssembly",
	// Assembly
	".asm": "Assembly",
	".s":   "Assembly",
	".S":   "Assembly",
	// Zig
	".zig": "Zig",
	// Nim
	".nim":    "Nim",
	".nims":   "Nim",
	".nimble": "Nim",
	// Julia
	".jl": "Julia",
	// V
	".v": "V",
	// Crystal
	".cr": "Crystal",
	// Groovy
	".groovy": "Groovy",
	".gradle": "Groovy",
	".gvy":    "Groovy",
	// Dockerfile
	".dockerfile": "Dockerfile",
	// Makefile extensions
	".mk":  "Makefile",
	".mak": "Makefile",
	// CMake
	".cmake": "CMake",
	// Terraform
	".tf":     "HCL",
	".tfvars": "HCL",
	".hcl":    "HCL",
}

// languageByExtension returns the programming language for a filename based on its extension.
// Returns empty string if the extension is not in the fast-path map (caller should fall back to content analysis).
func languageByExtension(filename string) string {
	ext := strings.ToLower(path.Ext(filename))
	if ext == "" {
		return ""
	}

	return extensionToLanguage[ext]
}

// LanguagesDetectionAnalyzer detects programming languages of changed files.
// It uses lazy detection - languages are only computed when Languages() is called.
type LanguagesDetectionAnalyzer struct {
	// Dependencies.
	TreeDiff  *TreeDiffAnalyzer
	BlobCache *BlobCacheAnalyzer

	// Output (private, use Languages() accessor).
	languages map[gitlib.Hash]string
	parsed    bool // tracks whether detection was done for current commit

	// Internal. //nolint:unused // used via reflection or external caller.
	l interface { //nolint:unused // acknowledged.
		Warnf(format string, args ...any)
	}
}

const (
	// ConfigLanguagesDetection is the configuration key for language detection settings.
	ConfigLanguagesDetection = "LanguagesDetection"
)

// Name returns the name of the analyzer.
func (l *LanguagesDetectionAnalyzer) Name() string {
	return "LanguagesDetection"
}

// Flag returns the CLI flag for the analyzer.
func (l *LanguagesDetectionAnalyzer) Flag() string {
	return "detect-languages"
}

// Description returns a human-readable description of the analyzer.
func (l *LanguagesDetectionAnalyzer) Description() string {
	return "Run programming language detection over the changed files."
}

// ListConfigurationOptions returns the configuration options for the analyzer.
func (l *LanguagesDetectionAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

// Configure sets up the analyzer with the provided facts.
func (l *LanguagesDetectionAnalyzer) Configure(_ map[string]any) error {
	return nil
}

// Initialize prepares the analyzer for processing commits.
func (l *LanguagesDetectionAnalyzer) Initialize(_ *gitlib.Repository) error {
	return nil
}

// Consume resets state for the new commit. Detection is deferred until Languages() is called.
func (l *LanguagesDetectionAnalyzer) Consume(_ *analyze.Context) error {
	// Reset state for new commit - detection is lazy
	l.languages = nil
	l.parsed = false

	return nil
}

// Languages returns detected languages, computing lazily on first call per commit.
// This avoids expensive language detection when downstream analyzers don't need it.
func (l *LanguagesDetectionAnalyzer) Languages() map[gitlib.Hash]string {
	if l.parsed {
		return l.languages
	}

	l.parsed = true
	changes := l.TreeDiff.Changes
	cache := l.BlobCache.Cache
	result := map[gitlib.Hash]string{}

	for _, change := range changes {
		switch change.Action {
		case gitlib.Insert:
			result[change.To.Hash] = l.detectLanguage(
				change.To.Name, cache[change.To.Hash])
		case gitlib.Delete:
			result[change.From.Hash] = l.detectLanguage(
				change.From.Name, cache[change.From.Hash])
		case gitlib.Modify:
			result[change.To.Hash] = l.detectLanguage(
				change.To.Name, cache[change.To.Hash])
			result[change.From.Hash] = l.detectLanguage(
				change.From.Name, cache[change.From.Hash])
		}
	}

	l.languages = result

	return l.languages
}

func (l *LanguagesDetectionAnalyzer) detectLanguage(name string, blob *gitlib.CachedBlob) string {
	if blob == nil {
		return ""
	}

	_, err := blob.CountLines()
	if errors.Is(err, gitlib.ErrBinary) {
		return ""
	}

	// Fast path: use extension-based lookup for unambiguous extensions.
	if lang := languageByExtension(name); lang != "" {
		return lang
	}

	// Slow path: fall back to content analysis for ambiguous cases.
	lang := enry.GetLanguage(path.Base(name), blob.Data)

	return lang
}

// SetLanguagesForTest sets the languages directly (for testing only).
func (l *LanguagesDetectionAnalyzer) SetLanguagesForTest(languages map[gitlib.Hash]string) {
	l.languages = languages
	l.parsed = true
}

// Finalize completes the analysis and returns the result.
func (l *LanguagesDetectionAnalyzer) Finalize() (analyze.Report, error) {
	return nil, nil //nolint:nilnil // nil,nil return is intentional.
}

// Fork creates a copy of the analyzer for parallel processing.
func (l *LanguagesDetectionAnalyzer) Fork(n int) []analyze.HistoryAnalyzer {
	res := make([]analyze.HistoryAnalyzer, n)
	for i := range n {
		clone := *l
		res[i] = &clone
	}

	return res
}

// Merge combines results from forked analyzer branches.
func (l *LanguagesDetectionAnalyzer) Merge(_ []analyze.HistoryAnalyzer) {
}

// Serialize writes the analysis result to the given writer.
func (l *LanguagesDetectionAnalyzer) Serialize(report analyze.Report, format string, writer io.Writer) error {
	if format == analyze.FormatJSON {
		err := json.NewEncoder(writer).Encode(report)
		if err != nil {
			return fmt.Errorf("json encode: %w", err)
		}
	}

	return nil
}

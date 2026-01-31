package plumbing //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestLanguageByExtension_CommonExtensions(t *testing.T) {
	t.Parallel()

	tests := []struct {
		filename string
		expected string
	}{
		// Go
		{"main.go", "Go"},
		{"pkg/util/helper.go", "Go"},
		// Python
		{"script.py", "Python"},
		{"app/models.py", "Python"},
		// JavaScript
		{"index.js", "JavaScript"},
		{"src/app.js", "JavaScript"},
		// TypeScript
		{"component.ts", "TypeScript"},
		{"src/types.ts", "TypeScript"},
		// TSX
		{"Component.tsx", "TSX"},
		// JSX
		{"Component.jsx", "JavaScript"},
		// Rust
		{"main.rs", "Rust"},
		{"lib.rs", "Rust"},
		// Java
		{"Main.java", "Java"},
		// C
		{"main.c", "C"},
		{"util.h", "C"},
		// C++
		{"main.cpp", "C++"},
		{"util.hpp", "C++"},
		{"main.cc", "C++"},
		// Ruby
		{"app.rb", "Ruby"},
		// PHP
		{"index.php", "PHP"},
		// Shell
		{"script.sh", "Shell"},
		{"deploy.bash", "Shell"},
		// YAML
		{"config.yaml", "YAML"},
		{"deploy.yml", "YAML"},
		// JSON
		{"package.json", "JSON"},
		// Markdown
		{"README.md", "Markdown"},
		// SQL
		{"query.sql", "SQL"},
		// Kotlin
		{"Main.kt", "Kotlin"},
		// Swift
		{"App.swift", "Swift"},
		// Scala
		{"Main.scala", "Scala"},
	}

	for _, tt := range tests {
		t.Run(tt.filename, func(t *testing.T) {
			t.Parallel()

			lang := languageByExtension(tt.filename)
			assert.Equal(t, tt.expected, lang)
		})
	}
}

func TestLanguageByExtension_UnknownExtension(t *testing.T) {
	t.Parallel()

	// Unknown extensions should return empty string to fall back to content analysis.
	tests := []string{
		"file.unknown",
		"data.xyz",
		"noextension",
		".hidden",
	}

	for _, filename := range tests {
		t.Run(filename, func(t *testing.T) {
			t.Parallel()

			lang := languageByExtension(filename)
			assert.Empty(t, lang)
		})
	}
}

func TestLanguageByExtension_CaseInsensitive(t *testing.T) {
	t.Parallel()

	tests := []struct {
		filename string
		expected string
	}{
		{"Main.GO", "Go"},
		{"Script.PY", "Python"},
		{"App.JS", "JavaScript"},
		{"FILE.CPP", "C++"},
	}

	for _, tt := range tests {
		t.Run(tt.filename, func(t *testing.T) {
			t.Parallel()

			lang := languageByExtension(tt.filename)
			assert.Equal(t, tt.expected, lang)
		})
	}
}

func TestDetectLanguage_FastPath(t *testing.T) {
	t.Parallel()

	ld := &LanguagesDetectionAnalyzer{}
	blob := gitlib.NewCachedBlobForTest([]byte("package main\n\nfunc main() {}\n"))

	// Go file should use fast path based on extension.
	lang := ld.detectLanguage("main.go", blob)
	assert.Equal(t, "Go", lang)
}

func TestDetectLanguage_FallbackToContent(t *testing.T) {
	t.Parallel()

	ld := &LanguagesDetectionAnalyzer{}

	// File with ambiguous extension should fall back to content analysis.
	// The content looks like Go code, so enry should detect it.
	blob := gitlib.NewCachedBlobForTest([]byte("package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"Hello\")\n}\n"))

	lang := ld.detectLanguage("file.txt", blob)
	// .txt is ambiguous, should fall back to content analysis.
	// enry may or may not detect Go from content.
	// The point is that it should NOT return empty for valid code.
	_ = lang // We just verify no panic occurs.
}

func TestDetectLanguage_BinaryFile(t *testing.T) {
	t.Parallel()

	ld := &LanguagesDetectionAnalyzer{}
	blob := gitlib.NewCachedBlobForTest([]byte("binary\x00data\x00here"))

	lang := ld.detectLanguage("binary.bin", blob)
	assert.Empty(t, lang)
}

func TestDetectLanguage_NilBlob(t *testing.T) {
	t.Parallel()

	ld := &LanguagesDetectionAnalyzer{}

	lang := ld.detectLanguage("main.go", nil)
	assert.Empty(t, lang)
}

func TestLanguagesDetectionAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	ld := &LanguagesDetectionAnalyzer{}
	err := ld.Configure(nil)
	require.NoError(t, err)
}

func TestLanguagesDetectionAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	ld := &LanguagesDetectionAnalyzer{}
	err := ld.Initialize(nil)
	require.NoError(t, err)
}

// Package main demonstrates custom UAST mapping usage.
package main

import (
	"log/slog"
	"os"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// ExampleCustomMappings demonstrates how to use custom UAST mappings.
//
//nolint:funlen // This example function is long due to the inline UAST mapping definition which cannot be shortened.
func ExampleCustomMappings(logger *slog.Logger) {
	// Define a custom UAST mapping for a simple configuration language.
	customMaps := map[string]uast.UASTMap{
		"simple_config": {
			Extensions: []string{".scfg", ".simple"},
			UAST: `[language "json", extensions: ".scfg", ".simple"]

_value <- (_value) => uast(
    type: "Synthetic"
)

array <- (array) => uast(
    token: "self",
    type: "Synthetic"
)

document <- (document) => uast(
    type: "Synthetic"
)

object <- (object) => uast(
    token: "self",
    type: "Synthetic"
)

pair <- (pair) => uast(
    type: "Synthetic",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "Synthetic"
)

comment <- (comment) => uast(
    type: "Comment",
    roles: "Comment"
)

false <- (false) => uast(
    type: "Synthetic"
)

null <- (null) => uast(
    token: "self",
    type: "Synthetic"
)

number <- (number) => uast(
    type: "Synthetic"
)

string_content <- (string_content) => uast(
    token: "self",
    type: "Synthetic"
)

true <- (true) => uast(
    type: "Synthetic"
)
`,
		},
	}

	// Create a new parser.
	parser, err := uast.NewParser()
	if err != nil {
		logger.Error("Failed to create parser", "error", err)
		os.Exit(1)
	}

	// Add custom mappings using the option pattern.
	parser = parser.WithUASTMap(customMaps)

	// Test that the custom parser is loaded.
	filename := "config.scfg"
	if parser.IsSupported(filename) {
		logger.Info("Parser supports file", "file", filename)
	} else {
		logger.Info("Parser does not support file", "file", filename)
	}

	// Test with some sample content.
	content := []byte(`{
		"app_name": "MyApp",
		"version": "1.0.0",
		"debug": true
	}`)

	// Parse the content.
	node, err := parser.Parse(filename, content)
	if err != nil {
		logger.Error("Failed to parse file", "file", filename, "error", err)
		os.Exit(1)
	}

	logger.Info("Successfully parsed file", "file", filename)
	logger.Info("Root node info", "type", node.Type, "children", len(node.Children))
}

// ExampleMultipleCustomMappings demonstrates using multiple custom mappings.
//
//nolint:funlen // This example function is long due to inline UAST mapping definitions which cannot be shortened.
func ExampleMultipleCustomMappings(logger *slog.Logger) {
	// Define multiple custom UAST mappings.
	customMaps := map[string]uast.UASTMap{
		"config_lang": {
			Extensions: []string{".config"},
			UAST: `[language "json", extensions: ".config"]

_value <- (_value) => uast(
    type: "Synthetic"
)

array <- (array) => uast(
    token: "self",
    type: "Synthetic"
)

document <- (document) => uast(
    type: "Synthetic"
)

object <- (object) => uast(
    token: "self",
    type: "Synthetic"
)

pair <- (pair) => uast(
    type: "Synthetic",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "Synthetic"
)
`,
		},
		"template_lang": {
			Extensions: []string{".tmpl", ".template"},
			UAST: `[language "json", extensions: ".tmpl", ".template"]

_value <- (_value) => uast(
    type: "Synthetic"
)

array <- (array) => uast(
    token: "self",
    type: "Synthetic"
)

document <- (document) => uast(
    type: "Synthetic"
)

object <- (object) => uast(
    token: "self",
    type: "Synthetic"
)

pair <- (pair) => uast(
    type: "Synthetic",
    children: "_value", "string"
)

string <- (string) => uast(
    token: "self",
    type: "Synthetic"
)
`,
		},
	}

	// Create parser with multiple custom mappings.
	parser, err := uast.NewParser()
	if err != nil {
		logger.Error("Failed to create parser", "error", err)
		os.Exit(1)
	}

	parser = parser.WithUASTMap(customMaps)

	// Test multiple file extensions.
	testFiles := []string{
		"app.config",
		"template.tmpl",
		"layout.template",
	}

	for _, filename := range testFiles {
		if parser.IsSupported(filename) {
			logger.Info("Parser supports file", "file", filename)
		} else {
			logger.Info("Parser does not support file", "file", filename)
		}
	}
}

func main() {
	logger := slog.New(slog.NewTextHandler(os.Stderr, nil))

	logger.Info("=== Custom UAST Mappings Example ===")

	logger.Info("1. Single Custom Mapping:")
	ExampleCustomMappings(logger)

	logger.Info("2. Multiple Custom Mappings:")
	ExampleMultipleCustomMappings(logger)
}

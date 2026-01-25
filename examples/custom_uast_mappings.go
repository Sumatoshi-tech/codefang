package main

import (
	"fmt"
	"log"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// ExampleCustomMappings demonstrates how to use custom UAST mappings
func ExampleCustomMappings() {
	// Define a custom UAST mapping for a simple configuration language
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

	// Create a new parser
	parser, err := uast.NewParser()
	if err != nil {
		log.Fatalf("Failed to create parser: %v", err)
	}

	// Add custom mappings using the option pattern
	parser = parser.WithUASTMap(customMaps)

	// Test that the custom parser is loaded
	filename := "config.scfg"
	if parser.IsSupported(filename) {
		fmt.Printf("✅ Parser supports %s\n", filename)
	} else {
		fmt.Printf("❌ Parser does not support %s\n", filename)
	}

	// Test with some sample content
	content := []byte(`{
		"app_name": "MyApp",
		"version": "1.0.0",
		"debug": true
	}`)

	// Parse the content
	node, err := parser.Parse(filename, content)
	if err != nil {
		log.Fatalf("Failed to parse %s: %v", filename, err)
	}

	fmt.Printf("✅ Successfully parsed %s\n", filename)
	fmt.Printf("   Root node type: %s\n", node.Type)
	fmt.Printf("   Number of children: %d\n", len(node.Children))
}

// ExampleMultipleCustomMappings demonstrates using multiple custom mappings
func ExampleMultipleCustomMappings() {
	// Define multiple custom UAST mappings
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

	// Create parser with multiple custom mappings
	parser, err := uast.NewParser()
	if err != nil {
		log.Fatalf("Failed to create parser: %v", err)
	}

	parser = parser.WithUASTMap(customMaps)

	// Test multiple file extensions
	testFiles := []string{
		"app.config",
		"template.tmpl",
		"layout.template",
	}

	for _, filename := range testFiles {
		if parser.IsSupported(filename) {
			fmt.Printf("✅ Parser supports %s\n", filename)
		} else {
			fmt.Printf("❌ Parser does not support %s\n", filename)
		}
	}
}

func main() {
	fmt.Println("=== Custom UAST Mappings Example ===")

	fmt.Println("1. Single Custom Mapping:")
	ExampleCustomMappings()

	fmt.Println("\n2. Multiple Custom Mappings:")
	ExampleMultipleCustomMappings()
}

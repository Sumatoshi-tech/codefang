package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"

	forest "github.com/alexaandru/go-sitter-forest"
	sitter "github.com/alexaandru/go-tree-sitter-bare"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/mapping"
	"github.com/spf13/cobra"
)

func mappingCmd() *cobra.Command {
	var nodeTypesPath, mappingPath, format, language, extensions string
	var coverage, generate, showTreeSitter bool

	cmd := &cobra.Command{
		Use:   "mapping",
		Short: "UAST mapping helpers: grammar analysis, classification, coverage",
		Long:  `Analyze node-types.json, classify nodes, compute mapping coverage, and show tree-sitter JSON structure.`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runMappingHelper(nodeTypesPath, mappingPath, format, coverage, generate, showTreeSitter, language, extensions, args)
		},
	}

	cmd.Flags().StringVar(&nodeTypesPath, "node-types", "", "Path to node-types.json (required for non-treesitter operations)")
	cmd.Flags().StringVar(&mappingPath, "mapping", "", "Path to mapping DSL file (optional)")
	cmd.Flags().StringVar(&format, "format", "text", "Output format: text or json")
	cmd.Flags().BoolVar(&coverage, "coverage", false, "Compute mapping coverage if mapping is provided")
	cmd.Flags().BoolVar(&generate, "generate", false, "Generate .uastmap DSL from node-types.json")
	cmd.Flags().BoolVar(&showTreeSitter, "show-treesitter", false, "Show original tree-sitter JSON structure for input files")
	cmd.Flags().StringVar(&language, "language", "", "Language for tree-sitter parsing (language name or grammar file path)")
	cmd.Flags().StringVar(&extensions, "extensions", "", "Comma-separated list of file extensions for language declaration")

	return cmd
}

func runMappingHelper(nodeTypesPath, mappingPath, format string, coverage, generate, showTreeSitter bool, language, extensions string, args []string) error {
	if showTreeSitter {
		return showTreeSitterJSON(args, language)
	}

	// Only require node-types when not showing tree-sitter JSON
	if nodeTypesPath == "" {
		return fmt.Errorf("--node-types is required for non-treesitter operations")
	}

	jsonData, err := os.ReadFile(nodeTypesPath)
	if err != nil {
		return fmt.Errorf("failed to read node-types.json: %w", err)
	}
	nodes, err := mapping.ParseNodeTypes(jsonData)
	if err != nil {
		return fmt.Errorf("failed to parse node-types.json: %w", err)
	}
	nodes = mapping.ApplyHeuristicClassification(nodes)

	if generate {
		// Parse extensions string into slice
		var extensionsSlice []string
		if extensions != "" {
			extensionsSlice = strings.Split(extensions, ",")
			// Trim spaces from each extension
			for i, ext := range extensionsSlice {
				extensionsSlice[i] = strings.TrimSpace(ext)
			}
		}

		dsl := mapping.GenerateMappingDSL(nodes, language, extensionsSlice)
		fmt.Print(dsl)
		return nil
	}

	var rules []mapping.MappingRule
	if mappingPath != "" {
		data, err := os.Open(mappingPath)
		if err != nil {
			return fmt.Errorf("failed to open mapping DSL: %w", err)
		}
		defer data.Close()
		_, _, err = (&mapping.MappingParser{}).ParseMapping(data)
		if err != nil {
			return fmt.Errorf("failed to load mapping DSL: %w", err)
		}
	}

	if format == "json" {
		out := map[string]interface{}{
			"node_count": len(nodes),
			"categories": summarizeCategories(nodes),
			"nodes":      nodes,
		}
		if coverage && len(rules) > 0 {
			cov, err := mapping.CoverageAnalysis(rules, nodes)
			if err != nil {
				return err
			}
			out["coverage"] = cov
		}
		enc := json.NewEncoder(os.Stdout)
		enc.SetIndent("", "  ")
		return enc.Encode(out)
	}

	fmt.Printf("Node types: %d\n", len(nodes))
	cats := summarizeCategories(nodes)
	for cat, count := range cats {
		fmt.Printf("  %s: %d\n", cat, count)
	}
	if coverage && len(rules) > 0 {
		cov, err := mapping.CoverageAnalysis(rules, nodes)
		if err != nil {
			return err
		}
		fmt.Printf("Coverage: %.2f%%\n", cov*100)
	}
	return nil
}

func showTreeSitterJSON(args []string, language string) error {
	if len(args) == 0 {
		return fmt.Errorf("no input files provided")
	}

	for _, filename := range args {
		if err := processFileForTreeSitterJSON(filename, language); err != nil {
			return fmt.Errorf("failed to process %s: %w", filename, err)
		}
	}
	return nil
}

func processFileForTreeSitterJSON(filename, language string) error {
	content, err := os.ReadFile(filename)
	if err != nil {
		return fmt.Errorf("failed to read file: %w", err)
	}

	// Create a parser
	parser := sitter.NewParser()

	// Set language if provided
	if language != "" {
		lang := forest.GetLanguage(language)
		parser.SetLanguage(lang)
	}

	// Try to parse
	tree, err := parser.ParseString(context.Background(), nil, content)
	if err != nil {
		if language == "" {
			return fmt.Errorf("tree-sitter parsing requires a language to be set. Error: %w\n\nUse --language flag to specify a language name or grammar file path", err)
		}
		return fmt.Errorf("failed to parse with tree-sitter: %w", err)
	}

	root := tree.RootNode()
	if root.IsNull() {
		return fmt.Errorf("no root node found")
	}

	jsonTree := convertTreeSitterNodeToJSON(root, content)

	fmt.Printf("=== Tree-sitter JSON for %s (language: %s) ===\n", filename, language)
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	if err := enc.Encode(jsonTree); err != nil {
		return fmt.Errorf("failed to encode JSON: %w", err)
	}
	fmt.Println()

	return nil
}

func convertTreeSitterNodeToJSON(node sitter.Node, source []byte) map[string]interface{} {
	result := map[string]interface{}{
		"type": node.Type(),
		"start_pos": map[string]int{
			"row":    int(node.StartPoint().Row),
			"column": int(node.StartPoint().Column),
		},
		"end_pos": map[string]int{
			"row":    int(node.EndPoint().Row),
			"column": int(node.EndPoint().Column),
		},
		"start_byte": int(node.StartByte()),
		"end_byte":   int(node.EndByte()),
	}

	if node.IsNamed() {
		result["named"] = true
	} else {
		result["named"] = false
	}

	// Extract text content
	text := node.Content(source)
	if text != "" {
		result["text"] = text
	}

	// Process children
	childCount := node.NamedChildCount()
	if childCount > 0 {
		children := make([]map[string]interface{}, 0, childCount)
		for i := uint32(0); i < childCount; i++ {
			child := node.NamedChild(i)
			if !child.IsNull() {
				children = append(children, convertTreeSitterNodeToJSON(child, source))
			}
		}
		if len(children) > 0 {
			result["children"] = children
		}
	}

	return result
}

func summarizeCategories(nodes []mapping.NodeTypeInfo) map[string]int {
	cats := map[string]int{}
	for _, n := range nodes {
		cats[fmt.Sprintf("%v", n.Category)]++
	}
	return cats
}

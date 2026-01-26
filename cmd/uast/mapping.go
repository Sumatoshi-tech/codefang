package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"strings"

	forest "github.com/alexaandru/go-sitter-forest"
	sitter "github.com/alexaandru/go-tree-sitter-bare"
	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/mapping"
)

// coveragePercent is the multiplier to convert a coverage ratio to a percentage.
const coveragePercent = 100

// Sentinel errors for the mapping command.
var (
	ErrNodeTypesRequired = errors.New("--node-types is required for non-treesitter operations")
	ErrNoInputFiles      = errors.New("no input files provided")
	ErrNoRootNode        = errors.New("no root node found")
)

func mappingCmd() *cobra.Command {
	var nodeTypesPath, mappingPath, format, language, extensions string

	var coverage, generate, showTreeSitter bool

	cmd := &cobra.Command{
		Use:   "mapping",
		Short: "UAST mapping helpers: grammar analysis, classification, coverage",
		Long:  `Analyze node-types.json, classify nodes, compute mapping coverage, and show tree-sitter JSON structure.`,
		RunE: func(_ *cobra.Command, args []string) error {
			return runMappingHelper(
				nodeTypesPath, mappingPath, format, coverage, generate,
				showTreeSitter, language, extensions, args,
			)
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

func runMappingHelper(
	nodeTypesPath, mappingPath, format string,
	coverage, generate, showTreeSitter bool,
	language, extensions string, args []string,
) error {
	if showTreeSitter {
		return showTreeSitterJSON(args, language)
	}

	nodes, err := loadNodeTypes(nodeTypesPath)
	if err != nil {
		return err
	}

	if generate {
		return runMappingGenerate(nodes, language, extensions)
	}

	rules, err := loadMappingRules(mappingPath)
	if err != nil {
		return err
	}

	if format == formatJSON {
		return outputMappingJSON(nodes, rules, coverage)
	}

	return outputMappingText(nodes, rules, coverage)
}

func loadNodeTypes(nodeTypesPath string) ([]mapping.NodeTypeInfo, error) {
	if nodeTypesPath == "" {
		return nil, ErrNodeTypesRequired
	}

	jsonData, err := os.ReadFile(nodeTypesPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read node-types.json: %w", err)
	}

	nodes, err := mapping.ParseNodeTypes(jsonData)
	if err != nil {
		return nil, fmt.Errorf("failed to parse node-types.json: %w", err)
	}

	return mapping.ApplyHeuristicClassification(nodes), nil
}

func runMappingGenerate(nodes []mapping.NodeTypeInfo, language, extensions string) error {
	var extensionsSlice []string

	if extensions != "" {
		extensionsSlice = strings.Split(extensions, ",")

		for idx, ext := range extensionsSlice {
			extensionsSlice[idx] = strings.TrimSpace(ext)
		}
	}

	dsl := mapping.GenerateMappingDSL(nodes, language, extensionsSlice)
	fmt.Fprint(os.Stdout, dsl)

	return nil
}

func loadMappingRules(mappingPath string) ([]mapping.MappingRule, error) {
	var rules []mapping.MappingRule

	if mappingPath != "" {
		data, openErr := os.Open(mappingPath)
		if openErr != nil {
			return nil, fmt.Errorf("failed to open mapping DSL: %w", openErr)
		}
		defer data.Close()

		_, _, parseErr := (&mapping.MappingParser{}).ParseMapping(data)
		if parseErr != nil {
			return nil, fmt.Errorf("failed to load mapping DSL: %w", parseErr)
		}
	}

	return rules, nil
}

func outputMappingJSON(nodes []mapping.NodeTypeInfo, rules []mapping.MappingRule, coverage bool) error {
	out := map[string]any{
		"node_count": len(nodes),
		"categories": summarizeCategories(nodes),
		"nodes":      nodes,
	}

	if coverage && len(rules) > 0 {
		covResult, covErr := mapping.CoverageAnalysis(rules, nodes)
		if covErr != nil {
			return covErr
		}

		out["coverage"] = covResult
	}

	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")

	err := enc.Encode(out)
	if err != nil {
		return fmt.Errorf("failed to encode JSON: %w", err)
	}

	return nil
}

func outputMappingText(nodes []mapping.NodeTypeInfo, rules []mapping.MappingRule, coverage bool) error {
	fmt.Fprintf(os.Stdout, "Node types: %d\n", len(nodes))

	cats := summarizeCategories(nodes)

	for cat, count := range cats {
		fmt.Fprintf(os.Stdout, "  %s: %d\n", cat, count)
	}

	if coverage && len(rules) > 0 {
		covResult, covErr := mapping.CoverageAnalysis(rules, nodes)
		if covErr != nil {
			return covErr
		}

		fmt.Fprintf(os.Stdout, "Coverage: %.2f%%\n", covResult*coveragePercent)
	}

	return nil
}

func showTreeSitterJSON(args []string, language string) error {
	if len(args) == 0 {
		return ErrNoInputFiles
	}

	for _, filename := range args {
		processErr := processFileForTreeSitterJSON(filename, language)
		if processErr != nil {
			return fmt.Errorf("failed to process %s: %w", filename, processErr)
		}
	}

	return nil
}

func processFileForTreeSitterJSON(filename, language string) error {
	content, err := os.ReadFile(filename)
	if err != nil {
		return fmt.Errorf("failed to read file: %w", err)
	}

	// Create a parser.
	parser := sitter.NewParser()

	// Set language if provided.
	if language != "" {
		lang := forest.GetLanguage(language)
		parser.SetLanguage(lang)
	}

	// Try to parse.
	tree, parseErr := parser.ParseString(context.Background(), nil, content)
	if parseErr != nil {
		if language == "" {
			return fmt.Errorf(
				"tree-sitter parsing requires a language to be set. Error: %w\n\n"+
					"Use --language flag to specify a language name or grammar file path", parseErr,
			)
		}

		return fmt.Errorf("failed to parse with tree-sitter: %w", parseErr)
	}

	root := tree.RootNode()
	if root.IsNull() {
		return ErrNoRootNode
	}

	jsonTree := convertTreeSitterNodeToJSON(root, content)

	fmt.Fprintf(os.Stdout, "=== Tree-sitter JSON for %s (language: %s) ===\n", filename, language)

	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")

	encodeErr := enc.Encode(jsonTree)
	if encodeErr != nil {
		return fmt.Errorf("failed to encode JSON: %w", encodeErr)
	}

	fmt.Fprintln(os.Stdout)

	return nil
}

func convertTreeSitterNodeToJSON(tsNode sitter.Node, source []byte) map[string]any {
	result := map[string]any{
		"type": tsNode.Type(),
		"start_pos": map[string]int{
			"row":    int(tsNode.StartPoint().Row),    //nolint:gosec // tree-sitter coordinates fit in int
			"column": int(tsNode.StartPoint().Column), //nolint:gosec // tree-sitter coordinates fit in int
		},
		"end_pos": map[string]int{
			"row":    int(tsNode.EndPoint().Row),    //nolint:gosec // tree-sitter coordinates fit in int
			"column": int(tsNode.EndPoint().Column), //nolint:gosec // tree-sitter coordinates fit in int
		},
		"start_byte": int(tsNode.StartByte()), //nolint:gosec // tree-sitter byte offsets fit in int
		"end_byte":   int(tsNode.EndByte()),   //nolint:gosec // tree-sitter byte offsets fit in int
	}

	if tsNode.IsNamed() {
		result["named"] = true
	} else {
		result["named"] = false
	}

	// Extract text content.
	text := tsNode.Content(source)
	if text != "" {
		result["text"] = text
	}

	// Process children.
	childCount := tsNode.NamedChildCount()

	if childCount > 0 {
		children := make([]map[string]any, 0, childCount)

		for idx := range childCount {
			child := tsNode.NamedChild(idx)

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

	for _, nodeInfo := range nodes {
		cats[fmt.Sprintf("%v", nodeInfo.Category)]++
	}

	return cats
}

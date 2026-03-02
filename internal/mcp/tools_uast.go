//go:build ignore
// +build ignore

package mcp

import (
	"context"
	"fmt"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// handleUASTParse processes uast_parse tool calls.
func handleUASTParse(
	ctx context.Context,
	_ *mcpsdk.CallToolRequest,
	input UASTParseInput,
) (*mcpsdk.CallToolResult, ToolOutput, error) {
	err := validateCodeInput(input.Code, input.Language)
	if err != nil {
		return errorResult(err)
	}

	parser, err := uast.NewParser()
	if err != nil {
		return errorResult(fmt.Errorf("create parser: %w", err))
	}

	filename := syntheticFilename(input.Language)

	if !parser.IsSupported(filename) {
		return errorResult(fmt.Errorf("%w: %s", ErrUnsupportedLanguage, input.Language))
	}

	root, err := parser.Parse(ctx, filename, []byte(input.Code))
	if err != nil {
		return errorResult(fmt.Errorf("parse code: %w", err))
	}

	if input.Query != "" {
		root = filterNodesByType(root, input.Query)
	}

	return jsonResult(root)
}

// filterNodesByType creates a filtered tree containing only nodes matching the query type.
func filterNodesByType(root *node.Node, nodeType string) *node.Node {
	var matches []*node.Node

	collectMatchingNodes(root, nodeType, &matches)

	return &node.Node{
		Type:     "filtered_results",
		Children: matches,
	}
}

// collectMatchingNodes walks the tree and collects nodes with matching type.
func collectMatchingNodes(current *node.Node, nodeType string, matches *[]*node.Node) {
	if current == nil {
		return
	}

	if string(current.Type) == nodeType {
		*matches = append(*matches, current)

		return
	}

	for _, child := range current.Children {
		collectMatchingNodes(child, nodeType, matches)
	}
}

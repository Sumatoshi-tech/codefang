//go:build ignore
// +build ignore

package mcp

import (
	"context"
	"fmt"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// defaultStaticAnalyzers returns all static analyzers for MCP analysis.
func defaultStaticAnalyzers() []analyze.StaticAnalyzer {
	return []analyze.StaticAnalyzer{
		complexity.NewAnalyzer(),
		comments.NewAnalyzer(),
		halstead.NewAnalyzer(),
		cohesion.NewAnalyzer(),
		imports.NewAnalyzer(),
	}
}

// allStaticAnalyzerNames returns the names of all default static analyzers.
func allStaticAnalyzerNames() []string {
	analyzers := defaultStaticAnalyzers()
	names := make([]string, 0, len(analyzers))

	for _, a := range analyzers {
		names = append(names, a.Name())
	}

	return names
}

// handleAnalyze processes codefang_analyze tool calls.
func handleAnalyze(
	ctx context.Context,
	_ *mcpsdk.CallToolRequest,
	input AnalyzeInput,
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

	analyzerNames := input.Analyzers
	if len(analyzerNames) == 0 {
		analyzerNames = allStaticAnalyzerNames()
	}

	factory := analyze.NewFactory(defaultStaticAnalyzers())

	results, err := factory.RunAnalyzers(ctx, root, analyzerNames)
	if err != nil {
		return errorResult(fmt.Errorf("run analyzers: %w", err))
	}

	return jsonResult(results)
}

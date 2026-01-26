package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Sentinel errors for the analyze command.
var (
	ErrNoFilesSpecified  = errors.New("no files specified for analysis")
	ErrUnsupportedAnaFmt = errors.New("unsupported format")
)

func analyzeCmd() *cobra.Command {
	var output, format string

	cmd := &cobra.Command{
		Use:   "analyze [files...]",
		Short: "Analyze code complexity and structure",
		Long: `Analyze source code for complexity, structure, and patterns.

Examples:
  uast analyze main.go                  # Analyze single file
  uast analyze *.go                     # Analyze all Go files
  uast analyze -f text *.go            # Text output format
  uast analyze -o report.html *.go     # Generate HTML report`,
		RunE: func(_ *cobra.Command, args []string) error {
			return runAnalyze(args, output, format)
		},
	}

	cmd.Flags().StringVarP(&output, "output", "o", "", "output file (default: stdout)")
	cmd.Flags().StringVarP(&format, "format", "f", "text", "output format (text, json, html)")

	return cmd
}

func runAnalyze(files []string, output, format string) error {
	if len(files) == 0 {
		return ErrNoFilesSpecified
	}

	var allResults []map[string]any

	for _, file := range files {
		parser, err := uast.NewParser()
		if err != nil {
			return fmt.Errorf("failed to initialize parser: %w", err)
		}

		if !parser.IsSupported(file) {
			fmt.Fprintf(os.Stderr, "Warning: Skipping unsupported file %s\n", file)

			continue
		}

		code, err := os.ReadFile(file)
		if err != nil {
			return fmt.Errorf("failed to read file %s: %w", file, err)
		}

		parsedNode, err := parser.Parse(file, code)
		if err != nil {
			return fmt.Errorf("parse error in %s: %w", file, err)
		}

		analysis := analyzeNode(parsedNode, file)
		allResults = append(allResults, analysis)
	}

	return outputAnalysis(allResults, output, format)
}

// analyzeNode produces analysis data for a parsed UAST node.
func analyzeNode(_ *node.Node, _ string) map[string]any {
	return map[string]any{}
}

//nolint:gocognit // output dispatch with format-specific logic is inherently complex
func outputAnalysis(results []map[string]any, output, format string) error {
	var writer io.Writer = os.Stdout

	if output != "" {
		outputFile, err := os.Create(output)
		if err != nil {
			return fmt.Errorf("failed to create output file: %w", err)
		}
		defer outputFile.Close()

		writer = outputFile
	}

	switch format {
	case formatJSON:
		enc := json.NewEncoder(writer)
		enc.SetIndent("", "  ")

		encodeErr := enc.Encode(results)
		if encodeErr != nil {
			return fmt.Errorf("failed to encode JSON: %w", encodeErr)
		}

		return nil
	case "text":
		for _, result := range results {
			fmt.Fprintf(writer, "File: %s\n", result["file"])
			fmt.Fprintf(writer, "  Functions: %d\n", result["functions"])
			fmt.Fprintf(writer, "  Complexity: %.2f\n", result["complexity"])

			types, ok := result["types"].(map[string]int)
			if !ok {
				continue
			}

			if len(types) > 0 {
				fmt.Fprintf(writer, "  Node types:\n")

				for nodeType, count := range types {
					fmt.Fprintf(writer, "    %s: %d\n", nodeType, count)
				}
			}

			fmt.Fprintf(writer, "\n")
		}

		return nil
	case "html":
		generateHTMLReport(results, writer)

		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedAnaFmt, format)
	}
}

func generateHTMLReport(results []map[string]any, writer io.Writer) {
	fmt.Fprintf(writer, "<!DOCTYPE html>\n<html>\n<head>\n<title>UAST Analysis Report</title>\n")
	fmt.Fprintf(writer, "<style>\nbody{font-family:Arial,sans-serif;margin:20px;}\n")
	fmt.Fprintf(writer, "table{border-collapse:collapse;width:100%%;}\n")
	fmt.Fprintf(writer, "th,td{border:1px solid #ddd;padding:8px;text-align:left;}\n")
	fmt.Fprintf(writer, "th{background-color:#f2f2f2;}\n</style>\n</head>\n<body>\n")

	fmt.Fprintf(writer, "<h1>UAST Analysis Report</h1>\n")
	fmt.Fprintf(writer, "<table>\n<tr><th>File</th><th>Functions</th><th>Complexity</th><th>Node Types</th></tr>\n")

	for _, result := range results {
		types, ok := result["types"].(map[string]int)
		if !ok {
			types = map[string]int{}
		}

		typeStr := ""

		for nodeType, count := range types {
			if typeStr != "" {
				typeStr += ", "
			}

			typeStr += fmt.Sprintf("%s: %d", nodeType, count)
		}

		fmt.Fprintf(writer, "<tr><td>%s</td><td>%d</td><td>%.2f</td><td>%s</td></tr>\n",
			result["file"], result["functions"], result["complexity"], typeStr)
	}

	fmt.Fprintf(writer, "</table>\n</body>\n</html>\n")
}

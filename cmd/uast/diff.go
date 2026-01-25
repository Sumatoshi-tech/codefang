package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/spf13/cobra"
)

func diffCmd() *cobra.Command {
	var output, format string
	var unified bool

	cmd := &cobra.Command{
		Use:   "diff file1 file2",
		Short: "Compare two files and detect changes",
		Long: `Compare two files and detect structural changes in their UAST.

Examples:
  uast diff file1.go file2.go          # Compare two files
  uast diff -u file1.go file2.go       # Unified diff format
  uast diff -f summary file1.go file2.go # Summary format`,
		Args: cobra.ExactArgs(2),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runDiff(args[0], args[1], output, format, unified)
		},
	}

	cmd.Flags().StringVarP(&output, "output", "o", "", "output file (default: stdout)")
	cmd.Flags().StringVarP(&format, "format", "f", "unified", "output format (unified, summary, json)")
	cmd.Flags().BoolVarP(&unified, "unified", "u", false, "unified diff format")

	return cmd
}

func runDiff(file1, file2, output, format string, unified bool) error {
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	// Parse first file
	var node1 *node.Node
	if parser.IsSupported(file1) {
		code, err := os.ReadFile(file1)
		if err != nil {
			return fmt.Errorf("failed to read file %s: %w", file1, err)
		}
		node1, err = parser.Parse(file1, code)
		if err != nil {
			return fmt.Errorf("parse error in %s: %w", file1, err)
		}
	} else {
		return fmt.Errorf("unsupported file type: %s", file1)
	}

	// Parse second file
	var node2 *node.Node
	if parser.IsSupported(file2) {
		code, err := os.ReadFile(file2)
		if err != nil {
			return fmt.Errorf("failed to read file %s: %w", file2, err)
		}
		node2, err = parser.Parse(file2, code)
		if err != nil {
			return fmt.Errorf("parse error in %s: %w", file2, err)
		}
	} else {
		return fmt.Errorf("unsupported file type: %s", file2)
	}

	// Detect changes
	changes := detectChanges(node1, node2, file1, file2)

	return outputChanges(changes, output, format, unified)
}

type Change struct {
	Type   string `json:"type"`
	File   string `json:"file"`
	Before any    `json:"before,omitempty"`
	After  any    `json:"after,omitempty"`
}

func detectChanges(node1, node2 *node.Node, file1, file2 string) []Change {
	// Use the uast package's optimized change detection
	uastChanges := uast.DetectChanges(node1, node2)

	// Convert uast.Change to local Change type
	changes := make([]Change, 0, len(uastChanges))
	for _, c := range uastChanges {
		changes = append(changes, Change{
			Type:   c.Type.String(),
			File:   file1,
			Before: c.Before,
			After:  c.After,
		})
	}

	return changes
}

func outputChanges(changes []Change, output, format string, unified bool) error {
	var writer io.Writer = os.Stdout
	if output != "" {
		f, err := os.Create(output)
		if err != nil {
			return fmt.Errorf("failed to create output file: %w", err)
		}
		defer f.Close()
		writer = f
	}

	switch format {
	case "json":
		enc := json.NewEncoder(writer)
		enc.SetIndent("", "  ")
		return enc.Encode(changes)
	case "unified":
		return printUnifiedDiff(changes, writer)
	case "summary":
		return printChangeSummary(changes, writer)
	default:
		return fmt.Errorf("unsupported format: %s", format)
	}
}

func printUnifiedDiff(changes []Change, writer io.Writer) error {
	for _, change := range changes {
		fmt.Fprintf(writer, "--- %s\n", change.File)
		fmt.Fprintf(writer, "+++ %s\n", change.File)
		fmt.Fprintf(writer, "@@ -1,1 +1,1 @@\n")
		fmt.Fprintf(writer, "-%s\n", change.Type)
		fmt.Fprintf(writer, "+%s\n", change.Type)
	}
	return nil
}

func printChangeSummary(changes []Change, writer io.Writer) error {
	summary := make(map[string]int)
	for _, change := range changes {
		summary[change.Type]++
	}

	fmt.Fprintf(writer, "Change Summary:\n")
	for changeType, count := range summary {
		fmt.Fprintf(writer, "  %s: %d\n", changeType, count)
	}
	return nil
}

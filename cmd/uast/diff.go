package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// diffArgCount is the number of arguments expected by the diff command.
const diffArgCount = 2

// Sentinel errors for the diff command.
var (
	ErrUnsupportedFileType = errors.New("unsupported file type")
	ErrUnsupportedDiffFmt  = errors.New("unsupported format")
)

func diffCmd() *cobra.Command {
	var output, format string

	cmd := &cobra.Command{
		Use:   "diff file1 file2",
		Short: "Compare two files and detect changes",
		Long: `Compare two files and detect structural changes in their UAST.

Examples:
  uast diff file1.go file2.go          # Compare two files
  uast diff -u file1.go file2.go       # Unified diff format
  uast diff -f summary file1.go file2.go # Summary format`,
		Args: cobra.ExactArgs(diffArgCount),
		RunE: func(_ *cobra.Command, args []string) error {
			return runDiff(args[0], args[1], output, format)
		},
	}

	cmd.Flags().StringVarP(&output, "output", "o", "", "output file (default: stdout)")
	cmd.Flags().StringVarP(&format, "format", "f", "unified", "output format (unified, summary, json)")

	return cmd
}

func runDiff(file1, file2, output, format string) error {
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	// Parse first file.
	if !parser.IsSupported(file1) {
		return fmt.Errorf("%w: %s", ErrUnsupportedFileType, file1)
	}

	code1, err := os.ReadFile(file1)
	if err != nil {
		return fmt.Errorf("failed to read file %s: %w", file1, err)
	}

	node1, err := parser.Parse(context.Background(), file1, code1)
	if err != nil {
		return fmt.Errorf("parse error in %s: %w", file1, err)
	}

	// Parse second file.
	if !parser.IsSupported(file2) {
		return fmt.Errorf("%w: %s", ErrUnsupportedFileType, file2)
	}

	code2, err := os.ReadFile(file2)
	if err != nil {
		return fmt.Errorf("failed to read file %s: %w", file2, err)
	}

	node2, err := parser.Parse(context.Background(), file2, code2)
	if err != nil {
		return fmt.Errorf("parse error in %s: %w", file2, err)
	}

	// Detect changes.
	changes := detectChanges(node1, node2, file1)

	return outputChanges(changes, output, format)
}

// Change represents a structural change between two UAST nodes.
type Change struct {
	Type   string `json:"type"`
	Before any    `json:"before,omitempty"`
	After  any    `json:"after,omitempty"`
	File   string `json:"file"`
}

func detectChanges(node1, node2 *node.Node, file1 string) []Change {
	// Use the uast package's optimized change detection.
	uastChanges := uast.DetectChanges(node1, node2)

	// Convert uast.Change to local Change type.
	changes := make([]Change, 0, len(uastChanges))

	for _, changeItem := range uastChanges {
		changes = append(changes, Change{
			Type:   changeItem.Type.String(),
			File:   file1,
			Before: changeItem.Before,
			After:  changeItem.After,
		})
	}

	return changes
}

func outputChanges(changes []Change, output, format string) error {
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

		encodeErr := enc.Encode(changes)
		if encodeErr != nil {
			return fmt.Errorf("failed to encode JSON: %w", encodeErr)
		}

		return nil
	case "unified":
		printUnifiedDiff(changes, writer)

		return nil
	case "summary":
		printChangeSummary(changes, writer)

		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedDiffFmt, format)
	}
}

func printUnifiedDiff(changes []Change, writer io.Writer) {
	for _, change := range changes {
		fmt.Fprintf(writer, "--- %s\n", change.File)
		fmt.Fprintf(writer, "+++ %s\n", change.File)
		fmt.Fprintf(writer, "@@ -1,1 +1,1 @@\n")
		fmt.Fprintf(writer, "-%s\n", change.Type)
		fmt.Fprintf(writer, "+%s\n", change.Type)
	}
}

func printChangeSummary(changes []Change, writer io.Writer) {
	summary := make(map[string]int)

	for _, change := range changes {
		summary[change.Type]++
	}

	fmt.Fprintf(writer, "Change Summary:\n")

	for changeType, count := range summary {
		fmt.Fprintf(writer, "  %s: %d\n", changeType, count)
	}
}

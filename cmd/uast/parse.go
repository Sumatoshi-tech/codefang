package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Sentinel errors for the parse command.
var (
	ErrNoSourceFiles       = errors.New("no source files found in the codebase")
	ErrUnsupportedParseFmt = errors.New("unsupported format")
)

func parseCmd() *cobra.Command {
	var lang, output, format string

	var progress, all bool

	cmd := &cobra.Command{
		Use:   "parse [files...]",
		Short: "Parse source code files into UAST",
		Long: `Parse source code files into Unified Abstract Syntax Tree (UAST) format.

Examples:
  uast parse main.go                    # Parse a single file
  uast parse *.go                       # Parse all Go files
  uast parse -l go main.c              # Force Go language for .c file
  cat main.go | uast parse -           # Parse from stdin
  uast parse -o output.json main.go    # Save to file
  uast parse -f json main.go           # Output as JSON
  uast parse --all                     # Parse all source files in the codebase`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runParse(args, lang, output, format, progress, all, cmd.OutOrStdout())
		},
	}

	cmd.Flags().StringVarP(&lang, "language", "l", "", "force language detection")
	cmd.Flags().StringVarP(&output, "output", "o", "", "output file (default: stdout)")
	cmd.Flags().StringVarP(&format, "format", "f", "json", "output format (json, compact, tree)")
	cmd.Flags().BoolVarP(&progress, "progress", "p", false, "show progress for multiple files")
	cmd.Flags().BoolVar(&all, "all", false, "parse all source files in the codebase recursively")

	return cmd
}

func runParse(files []string, lang, output, format string, progress, all bool, writer io.Writer) error {
	if all {
		var err error

		files, err = collectSourceFiles(".")
		if err != nil {
			return fmt.Errorf("failed to collect source files: %w", err)
		}

		if len(files) == 0 {
			return ErrNoSourceFiles
		}
	}

	if len(files) == 0 {
		// Read from stdin.
		return parseStdin(lang, output, format, writer)
	}

	if progress && len(files) > 1 {
		fmt.Fprintf(os.Stderr, "Parsing %d files...\n", len(files))
	}

	for idx, file := range files {
		if progress {
			fmt.Fprintf(os.Stderr, "[%d/%d] %s\n", idx+1, len(files), file)
		}

		parseErr := ParseFile(file, lang, output, format, writer)
		if parseErr != nil {
			return fmt.Errorf("failed to parse %s: %w", file, parseErr)
		}
	}

	return nil
}

func parseStdin(lang, output, format string, writer io.Writer) error {
	code, err := io.ReadAll(os.Stdin)
	if err != nil {
		return fmt.Errorf("failed to read stdin: %w", err)
	}

	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	filename := "stdin.go"
	if lang != "" {
		filename = "stdin." + lang
	}

	parsedNode, err := parser.Parse(context.Background(), filename, code)
	if err != nil {
		return fmt.Errorf("parse error: %w", err)
	}

	parsedNode.AssignStableIDs()

	return outputNode(parsedNode, output, format, writer)
}

// ParseFile parses a single source file into UAST format.
func ParseFile(file, lang, output, format string, writer io.Writer) error {
	code, err := os.ReadFile(file)
	if err != nil {
		return fmt.Errorf("failed to read file %s: %w", file, err)
	}

	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	filename := file
	if lang != "" {
		ext := filepath.Ext(file)
		filename = strings.TrimSuffix(file, ext) + "." + lang
	}

	parsedNode, err := parser.Parse(context.Background(), filename, code)
	if err != nil {
		return fmt.Errorf("parse error in %s: %w", file, err)
	}

	parsedNode.AssignStableIDs()

	return outputNode(parsedNode, output, format, writer)
}

func outputNode(parsedNode *node.Node, output, format string, writer io.Writer) error {
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

		encodeErr := enc.Encode(parsedNode.ToMap())
		if encodeErr != nil {
			return fmt.Errorf("failed to encode JSON: %w", encodeErr)
		}

		return nil
	case "compact":
		enc := json.NewEncoder(writer)

		encodeErr := enc.Encode(parsedNode.ToMap())
		if encodeErr != nil {
			return fmt.Errorf("failed to encode compact JSON: %w", encodeErr)
		}

		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedParseFmt, format)
	}
}

func collectSourceFiles(dir string) ([]string, error) {
	var files []string

	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		if !info.IsDir() {
			files = append(files, path)
		}

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("failed to walk directory: %w", err)
	}

	return files, nil
}

package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"sync/atomic"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/textutil"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

var (
	ErrNoSourceFiles       = errors.New("no source files found in the codebase")
	ErrUnsupportedParseFmt = errors.New("unsupported format")
)

const (
	formatNone    = "none"
	formatCompact = "compact"
)

func parseCmd() *cobra.Command {
	var (
		lang, output, format string
		workers              int
		progress, all        bool
	)

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
  uast parse -f none *.go              # Parse only, skip serialization
  uast parse --all                     # Parse all source files in the codebase
  uast parse --all -w 8                # Parse with 8 parallel workers`,
		RunE: func(cmd *cobra.Command, args []string) error {
			return runParse(args, lang, output, format, progress, all, workers, cmd.OutOrStdout())
		},
	}

	cmd.Flags().StringVarP(&lang, "language", "l", "", "force language detection")
	cmd.Flags().StringVarP(&output, "output", "o", "", "output file (default: stdout)")
	cmd.Flags().StringVarP(&format, "format", "f", "json", "output format (json, compact, tree, none)")
	cmd.Flags().BoolVarP(&progress, "progress", "p", false, "show progress for multiple files")
	cmd.Flags().BoolVar(&all, "all", false, "parse all source files in the codebase recursively")
	cmd.Flags().IntVarP(&workers, "workers", "w", 0, "number of parallel workers (default: number of CPUs)")

	return cmd
}

func runParse(files []string, lang, output, format string, progress, all bool, workers int, writer io.Writer) error {
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	files, err = resolveFiles(files, all, parser)
	if err != nil {
		return err
	}

	if len(files) == 0 {
		return parseStdin(lang, output, format, writer)
	}

	if progress && len(files) > 1 {
		fmt.Fprintf(os.Stderr, "Parsing %d files...\n", len(files))
	}

	if len(files) > 1 && format == formatNone {
		return runParseParallel(files, lang, progress, workers)
	}

	return parseFilesSequential(parser, files, lang, output, format, progress, writer)
}

func resolveFiles(files []string, all bool, parser *uast.Parser) ([]string, error) {
	if !all {
		return files, nil
	}

	collected, err := collectSourceFiles(".", parser)
	if err != nil {
		return nil, fmt.Errorf("failed to collect source files: %w", err)
	}

	if len(collected) == 0 {
		return nil, ErrNoSourceFiles
	}

	return collected, nil
}

func parseFilesSequential(parser *uast.Parser, files []string, lang, output, format string, progress bool, writer io.Writer) error {
	for idx, file := range files {
		if progress {
			fmt.Fprintf(os.Stderr, "[%d/%d] %s\n", idx+1, len(files), file)
		}

		parseErr := parseFileWithParser(parser, file, lang, output, format, writer)
		if parseErr != nil {
			return fmt.Errorf("failed to parse %s: %w", file, parseErr)
		}
	}

	return nil
}

// runParseParallel processes files concurrently using WorkerPool.
// Each goroutine reuses a Parser via [sync.Pool] to avoid contention.
func runParseParallel(files []string, lang string, progress bool, workers int) error {
	var (
		parserPool sync.Pool
		completed  atomic.Int64
		total      = int64(len(files))
	)

	pool := pipeline.WorkerPool[string]{
		MaxParallel: workers,
		Work: func(ctx context.Context, path string) error {
			p, err := getOrCreateParseParser(&parserPool)
			if err != nil {
				return err
			}
			defer parserPool.Put(p)

			parseErr := parseOnly(ctx, p, path, lang)
			if parseErr != nil {
				return fmt.Errorf("failed to parse %s: %w", path, parseErr)
			}

			if progress {
				done := completed.Add(1)
				fmt.Fprintf(os.Stderr, "[%d/%d] %s\n", done, total, path)
			}

			return nil
		},
	}

	return pool.Run(context.Background(), files)
}

// getOrCreateParseParser retrieves a parser from the pool or creates a new one.
func getOrCreateParseParser(pool *sync.Pool) (*uast.Parser, error) {
	if v := pool.Get(); v != nil {
		if p, ok := v.(*uast.Parser); ok {
			return p, nil
		}
	}

	return uast.NewParser()
}

// parseOnly parses a file without serialization — used in parallel mode.
func parseOnly(ctx context.Context, parser *uast.Parser, file, lang string) error {
	parsedNode, err := parser.ParseFile(ctx, file, lang)
	if err != nil {
		return err
	}

	runtime.KeepAlive(parsedNode)

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

func parseFileWithParser(parser *uast.Parser, file, lang, output, format string, writer io.Writer) error {
	parsedNode, err := parser.ParseFile(context.Background(), file, lang)
	if err != nil {
		return fmt.Errorf("failed to parse %s: %w", file, err)
	}

	if format == formatNone {
		runtime.KeepAlive(parsedNode)

		return nil
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
		return textutil.WriteJSON(writer, parsedNode.ToMap(), true)
	case formatCompact:
		return textutil.WriteJSON(writer, parsedNode.ToMap(), false)
	case formatNone:
		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedParseFmt, format)
	}
}

func collectSourceFiles(dir string, parser *uast.Parser) ([]string, error) {
	var files []string

	err := filepath.Walk(dir, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}

		if info.IsDir() {
			if isHiddenDir(filepath.Base(path)) {
				return filepath.SkipDir
			}

			return nil
		}

		if parser.IsSupported(path) {
			files = append(files, path)
		}

		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("failed to walk directory: %w", err)
	}

	return files, nil
}

// isHiddenDir returns true for directories that start with a dot (e.g. .git),
// except for "." and ".." which are filesystem navigation entries.
func isHiddenDir(name string) bool {
	return len(name) > 1 && name[0] == '.'
}

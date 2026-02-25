package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"

	"github.com/spf13/cobra"

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
	var lang, output, format string
	var workers int
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

	if all {
		var cerr error

		files, cerr = collectSourceFiles(".", parser)
		if cerr != nil {
			return fmt.Errorf("failed to collect source files: %w", cerr)
		}

		if len(files) == 0 {
			return ErrNoSourceFiles
		}
	}

	if len(files) == 0 {
		return parseStdin(lang, output, format, writer)
	}

	if progress && len(files) > 1 {
		fmt.Fprintf(os.Stderr, "Parsing %d files...\n", len(files))
	}

	useParallel := len(files) > 1 && format == formatNone
	if useParallel {
		return runParseParallel(parser, files, lang, format, progress, workers)
	}

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

// runParseParallel processes files concurrently using a worker pool.
// Each worker gets its own Parser instance to avoid contention.
func runParseParallel(sharedParser *uast.Parser, files []string, lang, format string, progress bool, workers int) error {
	if workers <= 0 {
		workers = runtime.NumCPU()
	}

	if workers > len(files) {
		workers = len(files)
	}

	fileCh := make(chan indexedFile, workers)
	var firstErr atomic.Value
	var completed atomic.Int64
	total := int64(len(files))

	var wg sync.WaitGroup

	for range workers {
		wg.Add(1)

		go func() {
			defer wg.Done()

			// Each goroutine creates its own parser to avoid contention on
			// tree-sitter parser pool and interner state.
			workerParser, perr := uast.NewParser()
			if perr != nil {
				firstErr.CompareAndSwap(nil, perr)
				return
			}

			for item := range fileCh {
				if firstErr.Load() != nil {
					return
				}

				perr := parseOnly(workerParser, item.path, lang)
				if perr != nil {
					firstErr.CompareAndSwap(nil, fmt.Errorf("failed to parse %s: %w", item.path, perr))

					return
				}

				done := completed.Add(1)
				if progress {
					fmt.Fprintf(os.Stderr, "[%d/%d] %s\n", done, total, item.path)
				}
			}
		}()
	}

	for i, f := range files {
		if firstErr.Load() != nil {
			break
		}

		fileCh <- indexedFile{index: i, path: f}
	}

	close(fileCh)
	wg.Wait()

	if errVal := firstErr.Load(); errVal != nil {
		if err, ok := errVal.(error); ok {
			return err
		}
	}

	_ = sharedParser

	return nil
}

type indexedFile struct {
	index int
	path  string
}

// parseOnly parses a file without serialization — used in parallel mode
// where format is "none".
func parseOnly(parser *uast.Parser, file, lang string) error {
	code, resolvedPath, err := safeReadFile(file)
	if err != nil {
		return err
	}

	filename := resolvedPath
	if lang != "" {
		ext := filepath.Ext(resolvedPath)
		filename = strings.TrimSuffix(resolvedPath, ext) + "." + lang
	}

	parsedNode, err := parser.Parse(context.Background(), filename, code)
	if err != nil {
		return err
	}

	// Keep the node alive to prevent the compiler from optimizing away the parse.
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
	code, resolvedPath, err := safeReadFile(file)
	if err != nil {
		return fmt.Errorf("failed to read file %s: %w", file, err)
	}

	filename := resolvedPath
	if lang != "" {
		ext := filepath.Ext(resolvedPath)
		filename = strings.TrimSuffix(resolvedPath, ext) + "." + lang
	}

	parsedNode, err := parser.Parse(context.Background(), filename, code)
	if err != nil {
		return fmt.Errorf("parse error in %s: %w", file, err)
	}

	if format == formatNone {
		runtime.KeepAlive(parsedNode)

		return nil
	}

	parsedNode.AssignStableIDs()

	return outputNode(parsedNode, output, format, writer)
}

// ParseFile parses a single source file into UAST format.
func ParseFile(file, lang, output, format string, writer io.Writer) error {
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	return parseFileWithParser(parser, file, lang, output, format, writer)
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
	case formatCompact:
		enc := json.NewEncoder(writer)

		encodeErr := enc.Encode(parsedNode.ToMap())
		if encodeErr != nil {
			return fmt.Errorf("failed to encode compact JSON: %w", encodeErr)
		}

		return nil
	case formatNone:
		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedParseFmt, format)
	}
}

func collectSourceFiles(dir string, parser *uast.Parser) ([]string, error) {
	var files []string

	err := filepath.Walk(dir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
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
// No other directories are excluded; file filtering is handled by
// parser.IsSupported which checks registered language extensions.
func isHiddenDir(name string) bool {
	return len(name) > 1 && name[0] == '.'
}

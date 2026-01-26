package main

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Sentinel errors for the query command.
var (
	ErrQueryExprRequired = errors.New("query expression required")
	ErrUnsupportedQFmt   = errors.New("unsupported format")
)

func queryCmd() *cobra.Command {
	var input, output, format string

	var interactive bool

	cmd := &cobra.Command{
		Use:   "query [query] [files...]",
		Short: "Query UAST with DSL expressions",
		Long: `Query parsed UAST nodes using the functional DSL.

Examples:
  uast query "filter(.type == 'Function')" main.go     # Find all functions
  uast query "filter(.roles has 'Exported')" *.go      # Find exported items
  uast query "reduce(count)" main.go                   # Count all nodes
  uast query -i main.go                                # Interactive mode
  uast query "filter(.type == 'Call')" - < input.json  # Query from stdin`,
		RunE: func(cobraCmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return ErrQueryExprRequired
			}

			query := args[0]
			files := args[1:]

			return runQuery(query, files, input, output, format, interactive, cobraCmd.OutOrStdout())
		},
	}

	cmd.Flags().StringVarP(&input, "input", "i", "", "input file (UAST JSON or source code)")
	cmd.Flags().StringVarP(&output, "output", "o", "", "output file (default: stdout)")
	cmd.Flags().StringVarP(&format, "format", "f", "json", "output format (json, compact, count)")
	cmd.Flags().BoolVarP(&interactive, "interactive", "t", false, "interactive query mode")

	return cmd
}

func runQuery(query string, files []string, input, output, format string, interactive bool, writer io.Writer) error {
	if interactive {
		return runInteractiveQuery(input, writer)
	}

	if len(files) == 0 && input == "" {
		// Query from stdin.
		return queryStdin(query, output, format, writer)
	}

	// Query from files.
	for _, file := range files {
		queryErr := queryFile(file, query, output, format, writer)
		if queryErr != nil {
			return fmt.Errorf("failed to query %s: %w", file, queryErr)
		}
	}

	return nil
}

func queryStdin(query, output, format string, writer io.Writer) error {
	var parsedNode *node.Node

	dec := json.NewDecoder(os.Stdin)

	decodeErr := dec.Decode(&parsedNode)
	if decodeErr != nil {
		return fmt.Errorf("failed to decode UAST from stdin: %w", decodeErr)
	}

	results, err := parsedNode.FindDSL(query)
	if err != nil {
		return fmt.Errorf("query error: %w", err)
	}

	return outputResults(results, output, format, writer)
}

func queryFile(file, query, output, format string, writer io.Writer) error {
	parsedNode, err := parseFileForQuery(file)
	if err != nil {
		return err
	}

	results, findErr := parsedNode.FindDSL(query)
	if findErr != nil {
		return fmt.Errorf("query error: %w", findErr)
	}

	return outputResults(results, output, format, writer)
}

// parseFileForQuery loads a UAST node from a file, auto-detecting whether it is
// a serialized JSON tree or a source file that needs parsing.
func parseFileForQuery(file string) (*node.Node, error) {
	if isJSONFile(file) {
		parsedNode, err := loadUASTFromJSON(file)
		if err != nil {
			return nil, fmt.Errorf("failed to query %s: %w", file, err)
		}

		return parsedNode, nil
	}

	parser, err := uast.NewParser()
	if err != nil {
		return nil, fmt.Errorf("failed to initialize parser: %w", err)
	}

	if !parser.IsSupported(file) {
		parsedNode, loadErr := loadUASTFromJSON(file)
		if loadErr != nil {
			return nil, fmt.Errorf("failed to query %s: %w", file, loadErr)
		}

		return parsedNode, nil
	}

	code, readErr := os.ReadFile(file)
	if readErr != nil {
		return nil, fmt.Errorf("failed to read file %s: %w", file, readErr)
	}

	parsedNode, parseErr := parser.Parse(file, code)
	if parseErr != nil {
		return nil, fmt.Errorf("parse error in %s: %w", file, parseErr)
	}

	return parsedNode, nil
}

func runInteractiveQuery(input string, _ io.Writer) error {
	parsedNode, err := loadInteractiveInput(input)
	if err != nil {
		return err
	}

	fmt.Fprintln(os.Stdout, "Interactive UAST Query Mode")
	fmt.Fprintln(os.Stdout, "Type 'help' for DSL syntax, 'quit' to exit")
	fmt.Fprintln(os.Stdout)

	scanner := bufio.NewScanner(os.Stdin)

	for {
		fmt.Fprint(os.Stdout, "uast> ")

		if !scanner.Scan() {
			break
		}

		query := strings.TrimSpace(scanner.Text())
		if query == "" {
			continue
		}

		if query == "quit" || query == "exit" {
			break
		}

		if query == "help" {
			printDSLHelp()

			continue
		}

		executeInteractiveQuery(parsedNode, query)

		fmt.Fprintln(os.Stdout)
	}

	return nil
}

func loadInteractiveInput(input string) (*node.Node, error) {
	if input != "" {
		return loadInteractiveInputFromFile(input)
	}

	return loadInteractiveInputFromStdin()
}

func loadInteractiveInputFromFile(input string) (*node.Node, error) {
	parser, err := uast.NewParser()
	if err != nil {
		return nil, fmt.Errorf("failed to initialize parser: %w", err)
	}

	if parser.IsSupported(input) {
		code, readErr := os.ReadFile(input)
		if readErr != nil {
			return nil, fmt.Errorf("failed to read file %s: %w", input, readErr)
		}

		parsedNode, parseErr := parser.Parse(input, code)
		if parseErr != nil {
			return nil, fmt.Errorf("parse error in %s: %w", input, parseErr)
		}

		return parsedNode, nil
	}

	// Try to read as UAST JSON.
	jsonFile, openErr := os.Open(input)
	if openErr != nil {
		return nil, fmt.Errorf("failed to open file %s: %w", input, openErr)
	}
	defer jsonFile.Close()

	var parsedNode *node.Node

	dec := json.NewDecoder(jsonFile)

	decodeErr := dec.Decode(&parsedNode)
	if decodeErr != nil {
		return nil, fmt.Errorf("failed to decode UAST from %s: %w", input, decodeErr)
	}

	return parsedNode, nil
}

func loadInteractiveInputFromStdin() (*node.Node, error) {
	code, err := io.ReadAll(os.Stdin)
	if err != nil {
		return nil, fmt.Errorf("failed to read stdin: %w", err)
	}

	parser, err := uast.NewParser()
	if err != nil {
		return nil, fmt.Errorf("failed to initialize parser: %w", err)
	}

	parsedNode, err := parser.Parse("stdin.go", code)
	if err != nil {
		return nil, fmt.Errorf("parse error: %w", err)
	}

	return parsedNode, nil
}

func executeInteractiveQuery(parsedNode *node.Node, query string) {
	results, err := parsedNode.FindDSL(query)
	if err != nil {
		fmt.Fprintf(os.Stdout, "Error: %v\n", err)

		return
	}

	if len(results) == 0 {
		fmt.Fprintln(os.Stdout, "No results found")
	} else {
		fmt.Fprintf(os.Stdout, "Found %d results:\n", len(results))

		for idx, resultNode := range results {
			fmt.Fprintf(os.Stdout, "[%d] %s: %s\n", idx+1, resultNode.Type, resultNode.Token)
		}
	}
}

func outputResults(results []*node.Node, output, format string, writer io.Writer) error {
	outputWriter := writer

	if output != "" {
		outFile, err := os.Create(output)
		if err != nil {
			return fmt.Errorf("failed to create output file: %w", err)
		}
		defer outFile.Close()

		outputWriter = outFile
	}

	mapped := nodesToMap(results)

	switch format {
	case formatJSON:
		enc := json.NewEncoder(outputWriter)
		enc.SetIndent("", "  ")

		encodeErr := enc.Encode(mapped)
		if encodeErr != nil {
			return fmt.Errorf("failed to encode JSON: %w", encodeErr)
		}

		return nil
	case "compact":
		enc := json.NewEncoder(outputWriter)

		encodeErr := enc.Encode(mapped)
		if encodeErr != nil {
			return fmt.Errorf("failed to encode compact JSON: %w", encodeErr)
		}

		return nil
	case "count":
		count := 0

		if arr, isArr := mapped["results"].([]any); isArr {
			count = len(arr)
		}

		fmt.Fprintf(outputWriter, "%d\n", count)

		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedQFmt, format)
	}
}

func printDSLHelp() {
	fmt.Fprintln(os.Stdout, "DSL Syntax:")
	fmt.Fprintln(os.Stdout, "  filter(.type == \"Function\")     - Filter by node type")
	fmt.Fprintln(os.Stdout, "  filter(.type == \"Call\")         - Find function calls")
	fmt.Fprintln(os.Stdout, "  filter(.type == \"Identifier\")   - Find identifiers")
	fmt.Fprintln(os.Stdout, "  filter(.type == \"Literal\")      - Find literals")
	fmt.Fprintln(os.Stdout)
}

func isJSONFile(file string) bool {
	return strings.HasSuffix(strings.ToLower(file), ".json")
}

func loadUASTFromJSON(file string) (*node.Node, error) {
	jsonFile, err := os.Open(file)
	if err != nil {
		return nil, fmt.Errorf("failed to open file %s: %w", file, err)
	}
	defer jsonFile.Close()

	var parsedNode node.Node

	dec := json.NewDecoder(jsonFile)

	decodeErr := dec.Decode(&parsedNode)
	if decodeErr != nil {
		return nil, fmt.Errorf("failed to decode UAST from %s: %w", file, decodeErr)
	}

	return &parsedNode, nil
}

// nodesToMap converts a slice of nodes to a map for JSON output.
func nodesToMap(nodes []*node.Node) map[string]any {
	if len(nodes) == 0 {
		return map[string]any{"results": []any{}}
	}

	allLiterals := true

	for _, currentNode := range nodes {
		if currentNode.Type != "Literal" {
			allLiterals = false

			break
		}
	}

	if allLiterals {
		results := make([]any, len(nodes))

		for idx, currentNode := range nodes {
			results[idx] = currentNode.Token
		}

		return map[string]any{"results": results}
	}

	if len(nodes) == 1 {
		return map[string]any{"results": []any{nodes[0].ToMap()}}
	}

	results := make([]any, len(nodes))

	for idx, currentNode := range nodes {
		results[idx] = currentNode.ToMap()
	}

	return map[string]any{"results": results}
}

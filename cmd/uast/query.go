package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/spf13/cobra"
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
		RunE: func(cmd *cobra.Command, args []string) error {
			if len(args) == 0 {
				return fmt.Errorf("query expression required")
			}
			query := args[0]
			files := args[1:]
			return runQuery(query, files, input, output, format, interactive, cmd.OutOrStdout())
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
		// Query from stdin
		return queryStdin(query, output, format, writer)
	}

	// Query from files
	for _, file := range files {
		if err := queryFile(file, query, output, format, writer); err != nil {
			return fmt.Errorf("failed to query %s: %w", file, err)
		}
	}

	return nil
}

func queryStdin(query, output, format string, writer io.Writer) error {
	var n *node.Node
	dec := json.NewDecoder(os.Stdin)
	if err := dec.Decode(&n); err != nil {
		return fmt.Errorf("failed to decode UAST from stdin: %w", err)
	}
	results, err := n.FindDSL(query)
	if err != nil {
		return fmt.Errorf("query error: %w", err)
	}
	return outputResults(results, output, format, writer)
}

func queryFile(file, query, output, format string, writer io.Writer) error {
	var n *node.Node
	if isJSONFile(file) {
		// Treat .json files as serialized UAST trees
		var err error
		n, err = loadUASTFromJSON(file)
		if err != nil {
			return fmt.Errorf("failed to query %s: %w", file, err)
		}
	} else {
		parser, err := uast.NewParser()
		if err != nil {
			return fmt.Errorf("failed to initialize parser: %w", err)
		}
		if parser.IsSupported(file) {
			code, err := os.ReadFile(file)
			if err != nil {
				return fmt.Errorf("failed to read file %s: %w", file, err)
			}
			n, err = parser.Parse(file, code)
			if err != nil {
				return fmt.Errorf("parse error in %s: %w", file, err)
			}
		} else {
			n, err = loadUASTFromJSON(file)
			if err != nil {
				return fmt.Errorf("failed to query %s: %w", file, err)
			}
		}
	}
	results, err := n.FindDSL(query)
	if err != nil {
		return fmt.Errorf("query error: %w", err)
	}
	return outputResults(results, output, format, writer)
}

func runInteractiveQuery(input string, writer io.Writer) error {
	var node *node.Node

	if input != "" {
		// Load from file
		parser, err := uast.NewParser()
		if err != nil {
			return fmt.Errorf("failed to initialize parser: %w", err)
		}

		if parser.IsSupported(input) {
			code, err := os.ReadFile(input)
			if err != nil {
				return fmt.Errorf("failed to read file %s: %w", input, err)
			}

			node, err = parser.Parse(input, code)
			if err != nil {
				return fmt.Errorf("parse error in %s: %w", input, err)
			}
		} else {
			// Try to read as UAST JSON
			f, err := os.Open(input)
			if err != nil {
				return fmt.Errorf("failed to open file %s: %w", input, err)
			}
			defer f.Close()

			dec := json.NewDecoder(f)
			if err := dec.Decode(&node); err != nil {
				return fmt.Errorf("failed to decode UAST from %s: %w", input, err)
			}
		}
	} else {
		// Read from stdin
		code, err := io.ReadAll(os.Stdin)
		if err != nil {
			return fmt.Errorf("failed to read stdin: %w", err)
		}

		parser, err := uast.NewParser()
		if err != nil {
			return fmt.Errorf("failed to initialize parser: %w", err)
		}

		node, err = parser.Parse("stdin.go", code)
		if err != nil {
			return fmt.Errorf("parse error: %w", err)
		}
	}

	fmt.Println("Interactive UAST Query Mode")
	fmt.Println("Type 'help' for DSL syntax, 'quit' to exit")
	fmt.Println()

	scanner := bufio.NewScanner(os.Stdin)
	for {
		fmt.Print("uast> ")
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

		results, err := node.FindDSL(query)
		if err != nil {
			fmt.Printf("Error: %v\n", err)
			continue
		}

		if len(results) == 0 {
			fmt.Println("No results found")
		} else {
			fmt.Printf("Found %d results:\n", len(results))
			for i, n := range results {
				fmt.Printf("[%d] %s: %s\n", i+1, n.Type, n.Token)
			}
		}
		fmt.Println()
	}

	return nil
}

func outputResults(results []*node.Node, output, format string, writer io.Writer) error {
	var w io.Writer = writer
	if output != "" {
		f, err := os.Create(output)
		if err != nil {
			return fmt.Errorf("failed to create output file: %w", err)
		}
		defer f.Close()
		w = f
	}
	mapped := nodesToMap(results)
	switch format {
	case "json":
		enc := json.NewEncoder(w)
		enc.SetIndent("", "  ")
		return enc.Encode(mapped)
	case "compact":
		enc := json.NewEncoder(w)
		return enc.Encode(mapped)
	case "count":
		count := 0
		if arr, ok := mapped["results"].([]any); ok {
			count = len(arr)
		}
		fmt.Fprintf(w, "%d\n", count)
		return nil
	default:
		return fmt.Errorf("unsupported format: %s", format)
	}
}

func printDSLHelp() {
	fmt.Println("DSL Syntax:")
	fmt.Println("  filter(.type == \"Function\")     - Filter by node type")
	fmt.Println("  filter(.type == \"Call\")         - Find function calls")
	fmt.Println("  filter(.type == \"Identifier\")   - Find identifiers")
	fmt.Println("  filter(.type == \"Literal\")      - Find literals")
	fmt.Println()
}

func isJSONFile(file string) bool {
	return strings.HasSuffix(strings.ToLower(file), ".json")
}

func loadUASTFromJSON(file string) (*node.Node, error) {
	f, err := os.Open(file)
	if err != nil {
		return nil, fmt.Errorf("failed to open file %s: %w", file, err)
	}
	defer f.Close()
	var n node.Node
	dec := json.NewDecoder(f)
	if err := dec.Decode(&n); err != nil {
		return nil, fmt.Errorf("failed to decode UAST from %s: %w", file, err)
	}
	return &n, nil
}

// Copy nodesToMap from main.go to this file for now, to resolve undefined error.
func nodesToMap(nodes []*node.Node) map[string]any {
	if len(nodes) == 0 {
		return map[string]any{"results": []any{}}
	}
	allLiterals := true
	for _, node := range nodes {
		if node.Type != "Literal" {
			allLiterals = false
			break
		}
	}
	if allLiterals {
		results := make([]any, len(nodes))
		for i, node := range nodes {
			results[i] = node.Token
		}
		return map[string]any{"results": results}
	}
	if len(nodes) == 1 {
		return map[string]any{"results": []any{nodes[0].ToMap()}}
	}
	results := make([]any, len(nodes))
	for i, node := range nodes {
		results[i] = node.ToMap()
	}
	return map[string]any{"results": results}
}

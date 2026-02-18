package main

import (
	"bufio"
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// minExploreArgs is the minimum number of args for find/query subcommands.
const minExploreArgs = 2

// Sentinel errors for the explore command.
var (
	ErrUnsupportedExploreFile = errors.New("unsupported file type")
	ErrNoFileSpecified        = errors.New("no file specified for exploration")
)

func exploreCmd() *cobra.Command {
	var lang string

	cmd := &cobra.Command{
		Use:   "explore [file]",
		Short: "Interactive UAST exploration",
		Long: `Start an interactive session to explore UAST structure.

Examples:
  uast explore main.go                  # Explore a file
  uast explore -l go main.c            # Force language detection`,
		RunE: func(_ *cobra.Command, args []string) error {
			file := ""
			if len(args) > 0 {
				file = args[0]
			}

			return runExplore(file, lang)
		},
	}

	cmd.Flags().StringVarP(&lang, "language", "l", "", "force language detection")

	return cmd
}

func runExplore(file, lang string) error {
	if file == "" {
		return ErrNoFileSpecified
	}

	parsedNode, err := parseExploreFile(file, lang)
	if err != nil {
		return err
	}

	fmt.Fprintf(os.Stdout, "Exploring %s\n", file)
	fmt.Fprintln(os.Stdout, "Type 'help' for commands, 'quit' to exit")
	fmt.Fprintln(os.Stdout)

	return runExploreLoop(parsedNode)
}

func parseExploreFile(file, lang string) (*node.Node, error) {
	parser, err := uast.NewParser()
	if err != nil {
		return nil, fmt.Errorf("failed to initialize parser: %w", err)
	}

	if !parser.IsSupported(file) {
		return nil, fmt.Errorf("%w: %s", ErrUnsupportedExploreFile, file)
	}

	code, err := os.ReadFile(file)
	if err != nil {
		return nil, fmt.Errorf("failed to read file %s: %w", file, err)
	}

	filename := file
	if lang != "" {
		ext := filepath.Ext(file)
		filename = strings.TrimSuffix(file, ext) + "." + lang
	}

	parsedNode, err := parser.Parse(context.Background(), filename, code)
	if err != nil {
		return nil, fmt.Errorf("parse error in %s: %w", file, err)
	}

	return parsedNode, nil
}

func runExploreLoop(parsedNode *node.Node) error {
	scanner := bufio.NewScanner(os.Stdin)

	for {
		fmt.Fprint(os.Stdout, "explore> ")

		if !scanner.Scan() {
			break
		}

		cmdText := strings.TrimSpace(scanner.Text())
		if cmdText == "" {
			continue
		}

		if cmdText == "quit" || cmdText == "exit" {
			break
		}

		if cmdText == "help" {
			printExploreHelp()

			continue
		}

		parts := strings.Fields(cmdText)
		if len(parts) == 0 {
			continue
		}

		handleExploreParts(parts, parsedNode)

		fmt.Fprintln(os.Stdout)
	}

	return nil
}

func handleExploreParts(parts []string, parsedNode *node.Node) {
	switch parts[0] {
	case "tree":
		fmt.Fprintln(os.Stdout, "Tree command is not available in this version.")
	case "stats":
		printStats(parsedNode)
	case "find":
		if len(parts) < minExploreArgs {
			fmt.Fprintln(os.Stdout, "Usage: find <type>")

			return
		}

		findNodes(parsedNode, parts[1])
	case "query":
		if len(parts) < minExploreArgs {
			fmt.Fprintln(os.Stdout, "Usage: query <dsl-query>")

			return
		}

		query := strings.Join(parts[1:], " ")

		results, err := parsedNode.FindDSL(query)
		if err != nil {
			fmt.Fprintf(os.Stdout, "Error: %v\n", err)
		} else {
			fmt.Fprintf(os.Stdout, "Found %d results\n", len(results))

			for idx, result := range results {
				fmt.Fprintf(os.Stdout, "[%d] %s: %s\n", idx+1, result.Type, result.Token)
			}
		}
	default:
		fmt.Fprintf(os.Stdout, "Unknown command: %s\n", parts[0])
		fmt.Fprintln(os.Stdout, "Type 'help' for available commands")
	}
}

func printStats(rootNode *node.Node) {
	stats := make(map[string]int)
	totalNodes := 0

	iter := rootNode.PreOrder()
	for nd := range iter {
		stats[string(nd.Type)]++
		totalNodes++
	}

	fmt.Fprintf(os.Stdout, "Total nodes: %d\n", totalNodes)
	fmt.Fprintln(os.Stdout, "By type:")

	for nodeType, count := range stats {
		fmt.Fprintf(os.Stdout, "  %s: %d\n", nodeType, count)
	}
}

func findNodes(rootNode *node.Node, nodeType string) {
	query := fmt.Sprintf("filter(.type == %q)", nodeType)

	results, err := rootNode.FindDSL(query)
	if err != nil {
		fmt.Fprintf(os.Stdout, "Error: %v\n", err)

		return
	}

	fmt.Fprintf(os.Stdout, "Found %d nodes of type '%s':\n", len(results), nodeType)

	for idx, result := range results {
		fmt.Fprintf(os.Stdout, "[%d] %s: %s\n", idx+1, result.Type, result.Token)
	}
}

func printExploreHelp() {
	fmt.Fprintln(os.Stdout, "Available commands:")
	fmt.Fprintln(os.Stdout, "  tree                    - Show AST tree structure")
	fmt.Fprintln(os.Stdout, "  stats                   - Show node statistics")
	fmt.Fprintln(os.Stdout, "  find <type>             - Find nodes by type")
	fmt.Fprintln(os.Stdout, "  query <dsl-query>       - Execute DSL query")
	fmt.Fprintln(os.Stdout, "  help                    - Show this help")
	fmt.Fprintln(os.Stdout, "  quit                    - Exit exploration")
	fmt.Fprintln(os.Stdout)
}

package main

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/spf13/cobra"
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
		RunE: func(cmd *cobra.Command, args []string) error {
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

// TODO: Move runExplore, exploreNode, printExploreHelp logic here from main.go.

func runExplore(file, lang string) error {
	var node *node.Node

	if file != "" {
		parser, err := uast.NewParser()
		if err != nil {
			return fmt.Errorf("failed to initialize parser: %w", err)
		}

		if !parser.IsSupported(file) {
			return fmt.Errorf("unsupported file type: %s", file)
		}

		code, err := os.ReadFile(file)
		if err != nil {
			return fmt.Errorf("failed to read file %s: %w", file, err)
		}

		filename := file
		if lang != "" {
			ext := filepath.Ext(file)
			filename = strings.TrimSuffix(file, ext) + "." + lang
		}

		node, err = parser.Parse(filename, code)
		if err != nil {
			return fmt.Errorf("parse error in %s: %w", file, err)
		}
	} else {
		return fmt.Errorf("no file specified for exploration")
	}

	fmt.Printf("Exploring %s\n", file)
	fmt.Println("Type 'help' for commands, 'quit' to exit")
	fmt.Println()

	scanner := bufio.NewScanner(os.Stdin)
	for {
		fmt.Print("explore> ")
		if !scanner.Scan() {
			break
		}

		cmd := strings.TrimSpace(scanner.Text())
		if cmd == "" {
			continue
		}

		if cmd == "quit" || cmd == "exit" {
			break
		}

		if cmd == "help" {
			printExploreHelp()
			continue
		}

		parts := strings.Fields(cmd)
		if len(parts) == 0 {
			continue
		}

		switch parts[0] {
		case "tree":
			// This command is removed as per the edit hint.
			fmt.Println("Tree command is not available in this version.")
		case "stats":
			printStats(node)
		case "find":
			if len(parts) < 2 {
				fmt.Println("Usage: find <type>")
				continue
			}
			findNodes(node, parts[1])
		case "query":
			if len(parts) < 2 {
				fmt.Println("Usage: query <dsl-query>")
				continue
			}
			query := strings.Join(parts[1:], " ")
			results, err := node.FindDSL(query)
			if err != nil {
				fmt.Printf("Error: %v\n", err)
			} else {
				fmt.Printf("Found %d results\n", len(results))
				for i, result := range results {
					fmt.Printf("[%d] %s: %s\n", i+1, result.Type, result.Token)
				}
			}
		default:
			fmt.Printf("Unknown command: %s\n", parts[0])
			fmt.Println("Type 'help' for available commands")
		}
		fmt.Println()
	}

	return nil
}

func printStats(node *node.Node) {
	stats := make(map[string]int)
	totalNodes := 0

	iter := node.PreOrder()
	for n := range iter {
		stats[string(n.Type)]++
		totalNodes++
	}

	fmt.Printf("Total nodes: %d\n", totalNodes)
	fmt.Println("By type:")
	for nodeType, count := range stats {
		fmt.Printf("  %s: %d\n", nodeType, count)
	}
}
func findNodes(node *node.Node, nodeType string) {
	query := fmt.Sprintf("filter(.type == \"%s\")", nodeType)
	results, err := node.FindDSL(query)
	if err != nil {
		fmt.Printf("Error: %v\n", err)
		return
	}

	fmt.Printf("Found %d nodes of type '%s':\n", len(results), nodeType)
	for i, result := range results {
		fmt.Printf("[%d] %s: %s\n", i+1, result.Type, result.Token)
	}
}

func printExploreHelp() {
	fmt.Println("Available commands:")
	fmt.Println("  tree                    - Show AST tree structure")
	fmt.Println("  stats                   - Show node statistics")
	fmt.Println("  find <type>             - Find nodes by type")
	fmt.Println("  query <dsl-query>       - Execute DSL query")
	fmt.Println("  help                    - Show this help")
	fmt.Println("  quit                    - Exit exploration")
	fmt.Println()
}

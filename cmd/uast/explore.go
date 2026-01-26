package main

import (
	"bufio"
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

func runExplore( //nolint:gocognit // complex but clear interactive loop
	file, lang string,
) error { //nolint:whitespace // multi-line function signature requires blank line after opening brace
	if file == "" {
		return ErrNoFileSpecified
	}

	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	if !parser.IsSupported(file) {
		return fmt.Errorf("%w: %s", ErrUnsupportedExploreFile, file)
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

	parsedNode, err := parser.Parse(filename, code)
	if err != nil {
		return fmt.Errorf("parse error in %s: %w", file, err)
	}

	fmt.Printf("Exploring %s\n", file)                      //nolint:forbidigo // CLI user output
	fmt.Println("Type 'help' for commands, 'quit' to exit") //nolint:forbidigo // CLI user output
	fmt.Println()                                           //nolint:forbidigo // CLI user output

	scanner := bufio.NewScanner(os.Stdin)

	for {
		fmt.Print("explore> ") //nolint:forbidigo // CLI user output

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

		fmt.Println() //nolint:forbidigo // CLI user output
	}

	return nil
}

func handleExploreParts(parts []string, parsedNode *node.Node) {
	switch parts[0] {
	case "tree":
		fmt.Println("Tree command is not available in this version.") //nolint:forbidigo // CLI user output
	case "stats":
		printStats(parsedNode)
	case "find":
		if len(parts) < minExploreArgs {
			fmt.Println("Usage: find <type>") //nolint:forbidigo // CLI user output

			return
		}

		findNodes(parsedNode, parts[1])
	case "query":
		if len(parts) < minExploreArgs {
			fmt.Println("Usage: query <dsl-query>") //nolint:forbidigo // CLI user output

			return
		}

		query := strings.Join(parts[1:], " ")

		results, err := parsedNode.FindDSL(query)
		if err != nil {
			fmt.Printf("Error: %v\n", err) //nolint:forbidigo // CLI user output
		} else {
			fmt.Printf("Found %d results\n", len(results)) //nolint:forbidigo // CLI user output

			for idx, result := range results {
				fmt.Printf("[%d] %s: %s\n", idx+1, result.Type, result.Token) //nolint:forbidigo // CLI user output
			}
		}
	default:
		fmt.Printf("Unknown command: %s\n", parts[0])     //nolint:forbidigo // CLI user output
		fmt.Println("Type 'help' for available commands") //nolint:forbidigo // CLI user output
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

	fmt.Printf("Total nodes: %d\n", totalNodes) //nolint:forbidigo // CLI user output
	fmt.Println("By type:")                     //nolint:forbidigo // CLI user output

	for nodeType, count := range stats {
		fmt.Printf("  %s: %d\n", nodeType, count) //nolint:forbidigo // CLI user output
	}
}

func findNodes(rootNode *node.Node, nodeType string) {
	query := fmt.Sprintf("filter(.type == %q)", nodeType)

	results, err := rootNode.FindDSL(query)
	if err != nil {
		fmt.Printf("Error: %v\n", err) //nolint:forbidigo // CLI user output

		return
	}

	fmt.Printf("Found %d nodes of type '%s':\n", len(results), nodeType) //nolint:forbidigo // CLI user output

	for idx, result := range results {
		fmt.Printf("[%d] %s: %s\n", idx+1, result.Type, result.Token) //nolint:forbidigo // CLI user output
	}
}

func printExploreHelp() {
	fmt.Println("Available commands:")                                 //nolint:forbidigo // CLI user output
	fmt.Println("  tree                    - Show AST tree structure") //nolint:forbidigo // CLI user output
	fmt.Println("  stats                   - Show node statistics")    //nolint:forbidigo // CLI user output
	fmt.Println("  find <type>             - Find nodes by type")      //nolint:forbidigo // CLI user output
	fmt.Println("  query <dsl-query>       - Execute DSL query")       //nolint:forbidigo // CLI user output
	fmt.Println("  help                    - Show this help")          //nolint:forbidigo // CLI user output
	fmt.Println("  quit                    - Exit exploration")        //nolint:forbidigo // CLI user output
	fmt.Println()                                                      //nolint:forbidigo // CLI user output
}

// Package main provides the entry point for the codefang CLI tool.
package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/cmd/codefang/commands"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

var (
	verbose bool //nolint:gochecknoglobals // CLI flag variable
	quiet   bool //nolint:gochecknoglobals // CLI flag variable
)

func main() {
	rootCmd := &cobra.Command{
		Use:   "codefang",
		Short: "Codefang Code Analysis - Unified code analysis tool",
		Long: `Codefang provides comprehensive code analysis tools.

Commands:
  analyze   Static analysis on UAST output (complexity, cohesion, comments, halstead, imports)
  history   Git repository history analysis (burndown, couples, devs, file-history, imports, sentiment, shotness, typos)

Static Analysis Examples:
  uast parse main.go | codefang analyze                        # Analyze single file
  uast parse main.go | codefang analyze -a complexity,halstead # Specific analyzers
  uast parse *.go | codefang analyze -f json                   # JSON output

History Analysis Examples:
  codefang history -a burndown .                               # Burndown analysis
  codefang history -a burndown,couples,devs .                  # Multiple analyzers
  codefang history -a devs --head .                            # Latest commit only`,
		SilenceUsage:  true,
		SilenceErrors: true,
	}

	rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "verbose output")
	rootCmd.PersistentFlags().BoolVarP(&quiet, "quiet", "q", false, "suppress output")

	// Add commands.
	rootCmd.AddCommand(commands.NewAnalyzeCommand())
	rootCmd.AddCommand(commands.NewHistoryCommand())
	rootCmd.AddCommand(versionCmd())

	err := rootCmd.Execute()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func versionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Show version information",
		Run: func(_ *cobra.Command, _ []string) {
			fmt.Fprintf(os.Stdout, "codefang %s (commit: %s, built: %s)\n", version.Version, version.Commit, version.Date)
		},
	}
}

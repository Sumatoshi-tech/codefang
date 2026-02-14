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
	verbose bool
	quiet   bool
)

func main() {
	version.InitBinaryVersion()

	rootCmd := &cobra.Command{
		Use:   "codefang",
		Short: "Codefang Code Analysis - Unified code analysis tool",
		Long: `Codefang provides comprehensive code analysis tools.

Commands:
  run       Unified static + history analysis entrypoint`,
		SilenceUsage:  true,
		SilenceErrors: true,
	}

	rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "verbose output")
	rootCmd.PersistentFlags().BoolVarP(&quiet, "quiet", "q", false, "suppress output")

	// Add commands.
	rootCmd.AddCommand(commands.NewRunCommand())
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

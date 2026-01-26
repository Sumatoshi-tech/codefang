// Package main provides the UAST CLI entry point.
package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// formatJSON is the constant for the "json" output format string.
const formatJSON = "json"

var (
	cfgFile string //nolint:gochecknoglobals // CLI flag variable
	verbose bool   //nolint:gochecknoglobals // CLI flag variable
	quiet   bool   //nolint:gochecknoglobals // CLI flag variable
)

func main() {
	rootCmd := &cobra.Command{
		Use:   "uast",
		Short: "UAST (Universal Abstract Syntax Tree) parser and analyzer",
		Long:  `UAST is a tool for parsing source code into Universal Abstract Syntax Trees.`,
	}

	rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default is $HOME/.uast.yaml)")
	rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "verbose output")
	rootCmd.PersistentFlags().BoolVarP(&quiet, "quiet", "q", false, "suppress output")

	rootCmd.AddCommand(parseCmd())
	rootCmd.AddCommand(diffCmd())
	rootCmd.AddCommand(queryCmd())
	rootCmd.AddCommand(exploreCmd())
	rootCmd.AddCommand(analyzeCmd())
	rootCmd.AddCommand(completionCmd())
	rootCmd.AddCommand(versionCmd())
	rootCmd.AddCommand(validateCmd())
	rootCmd.AddCommand(mappingCmd())
	rootCmd.AddCommand(lspCmd())
	rootCmd.AddCommand(serverCmd())

	err := rootCmd.Execute()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func versionCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "version",
		Short: "Show version information",
		Run: func(_ *cobra.Command, _ []string) {
			fmt.Fprintf(os.Stdout, "uast %s (commit: %s, built: %s)\n", version.Version, version.Commit, version.Date)
		},
	}

	return cmd
}

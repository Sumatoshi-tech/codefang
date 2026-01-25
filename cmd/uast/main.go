package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var (
	cfgFile string
	verbose bool
	quiet   bool
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

	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintf(os.Stderr, "Error: %v\n", err)
		os.Exit(1)
	}
}

func versionCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "version",
		Short: "Show version information",
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Printf("UAST CLI v0.1.0\n")
			fmt.Printf("Go version: %s\n", "1.22")
		},
	}

	return cmd
}

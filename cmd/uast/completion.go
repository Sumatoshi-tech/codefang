package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

func completionCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "completion [shell]",
		Short: "Generate shell completion scripts",
		Long: `Generate shell completion scripts for uast.

Examples:
  uast completion bash                  # Generate bash completion
  uast completion zsh                   # Generate zsh completion
  uast completion fish                  # Generate fish completion`,
		Args: cobra.ExactArgs(1),
		RunE: func(cmd *cobra.Command, args []string) error {
			return runCompletion(args[0])
		},
	}

	return cmd
}

func runCompletion(shell string) error {
	rootCmd := &cobra.Command{
		Use:   "uast",
		Short: "Unified AST - Parse, analyze, and transform code across 100+ languages",
	}

	rootCmd.AddCommand(parseCmd())
	rootCmd.AddCommand(queryCmd())
	rootCmd.AddCommand(analyzeCmd())
	rootCmd.AddCommand(diffCmd())
	rootCmd.AddCommand(exploreCmd())
	rootCmd.AddCommand(completionCmd())
	rootCmd.AddCommand(versionCmd())

	switch shell {
	case "bash":
		return rootCmd.GenBashCompletion(os.Stdout)
	case "zsh":
		return rootCmd.GenZshCompletion(os.Stdout)
	case "fish":
		return rootCmd.GenFishCompletion(os.Stdout, true)
	case "powershell":
		return rootCmd.GenPowerShellCompletion(os.Stdout)
	default:
		return fmt.Errorf("unsupported shell: %s", shell)
	}
}

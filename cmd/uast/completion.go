package main

import (
	"errors"
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

// ErrUnsupportedShell is returned when an unsupported shell is specified.
var ErrUnsupportedShell = errors.New("unsupported shell")

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
		RunE: func(_ *cobra.Command, args []string) error {
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

	var err error

	switch shell {
	case "bash":
		err = rootCmd.GenBashCompletion(os.Stdout)
	case "zsh":
		err = rootCmd.GenZshCompletion(os.Stdout)
	case "fish":
		err = rootCmd.GenFishCompletion(os.Stdout, true)
	case "powershell":
		err = rootCmd.GenPowerShellCompletion(os.Stdout)
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedShell, shell)
	}

	if err != nil {
		return fmt.Errorf("failed to generate %s completion: %w", shell, err)
	}

	return nil
}

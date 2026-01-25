package main

import (
	"github.com/Sumatoshi-tech/codefang/pkg/uast/lsp"
	"github.com/spf13/cobra"
)

func lspCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "lsp",
		Short: "Start language server for mapping and query DSL (LSP)",
		Long:  `Start a language server (LSP) for .uastmap and query DSL files (stdio mode).`,
		RunE: func(cmd *cobra.Command, args []string) error {
			lsp.NewServer().Run()
			return nil
		},
	}
	return cmd
}

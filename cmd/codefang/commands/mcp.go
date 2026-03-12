//go:build ignore

package commands

import (
	"context"
	"log/slog"
	"os"

	"github.com/spf13/cobra"

	"github.com/Sumatoshi-tech/codefang/internal/mcp"
	"github.com/Sumatoshi-tech/codefang/internal/observability"
	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// NewMCPCommand creates the MCP server command.
func NewMCPCommand() *cobra.Command {
	var debug bool

	cmd := &cobra.Command{
		Use:   "mcp",
		Short: "Start MCP server for AI agent integration",
		Long: `Start a Model Context Protocol (MCP) server on stdio transport.

The MCP server exposes Codefang analysis capabilities as tools that AI agents
can discover and invoke:
  - codefang_analyze: Static code analysis (complexity, cohesion, halstead, comments, imports)
  - codefang_history: Git history analysis (burndown, couples, devs, sentiment, etc.)
  - uast_parse: Parse source code into Universal AST`,
		SilenceUsage:  true,
		SilenceErrors: true,
		RunE: func(cobraCmd *cobra.Command, _ []string) error {
			providers, err := initMCPObservability(debug)
			if err != nil {
				return err
			}

			defer func() {
				shutdownErr := providers.Shutdown(context.Background())
				if shutdownErr != nil {
					providers.Logger.Warn("observability shutdown failed", "error", shutdownErr)
				}
			}()

			red, redErr := observability.NewREDMetrics(providers.Meter)
			if redErr != nil {
				return redErr
			}

			deps := mcp.ServerDeps{Logger: providers.Logger, Metrics: red, Tracer: providers.Tracer}

			srv := mcp.NewServer(deps)

			return srv.Run(cobraCmd.Context())
		},
	}

	cmd.Flags().BoolVar(&debug, "debug", false, "Enable debug logging to stderr")

	return cmd
}

func initMCPObservability(debug bool) (observability.Providers, error) {
	cfg := observability.DefaultConfig()
	cfg.ServiceVersion = version.Version
	cfg.OTLPEndpoint = os.Getenv("OTEL_EXPORTER_OTLP_ENDPOINT")
	cfg.OTLPHeaders = observability.ParseOTLPHeaders(os.Getenv("OTEL_EXPORTER_OTLP_HEADERS"))
	cfg.OTLPInsecure = os.Getenv("OTEL_EXPORTER_OTLP_INSECURE") == "true"
	cfg.Mode = observability.ModeMCP
	cfg.LogJSON = true

	if debug {
		cfg.LogLevel = slog.LevelDebug
		cfg.DebugTrace = true
	}

	return observability.Init(cfg)
}

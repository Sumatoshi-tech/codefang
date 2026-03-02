// Package observability provides OpenTelemetry-based tracing, metrics, and
// structured logging for all Codefang application modes (CLI, MCP, server).
package observability

import "log/slog"

// AppMode identifies the application execution mode.
type AppMode string

const (
	// ModeCLI is the CLI command execution mode.
	ModeCLI AppMode = "cli"
	// ModeMCP is the MCP stdio server mode.
	ModeMCP AppMode = "mcp"
	// ModeServe is the HTTP/gRPC server mode.
	ModeServe AppMode = "serve"
)

const (
	// defaultServiceName is the default OTel service name.
	defaultServiceName = "codefang"

	// defaultShutdownTimeoutSec is the default shutdown timeout in seconds.
	defaultShutdownTimeoutSec = 5
)

// Config holds all observability configuration.
type Config struct {
	// ServiceName is the OTel resource service name.
	ServiceName string

	// ServiceVersion is the semantic version of the running binary.
	ServiceVersion string

	// Environment is the deployment environment (e.g. "production", "staging", "dev").
	Environment string

	// Mode identifies how the binary was launched.
	Mode AppMode

	// OTLPEndpoint is the OTLP gRPC collector address (e.g. "localhost:4317").
	// Empty disables export; providers become no-op.
	OTLPEndpoint string

	// OTLPHeaders are additional gRPC metadata headers for the OTLP exporter.
	OTLPHeaders map[string]string

	// OTLPInsecure disables TLS for the OTLP gRPC connection.
	OTLPInsecure bool

	// DebugTrace forces 100% trace sampling when true.
	DebugTrace bool

	// SampleRatio is the trace sampling ratio (0.0 to 1.0) when DebugTrace is false.
	// Zero uses the OTel SDK default (parent-based with always-on root).
	SampleRatio float64

	// LogLevel controls the minimum slog severity.
	LogLevel slog.Level

	// TraceVerbose enables hot-path spans (per-commit, per-file, per-git-op).
	// When false (default), only structural pipeline spans are recorded.
	TraceVerbose bool

	// LogJSON enables JSON-formatted log output.
	LogJSON bool

	// ShutdownTimeoutSec is the maximum seconds to wait for flush on shutdown.
	ShutdownTimeoutSec int
}

// DefaultConfig returns a Config with sensible defaults for zero-config startup.
func DefaultConfig() Config {
	return Config{
		ServiceName:        defaultServiceName,
		Mode:               ModeCLI,
		LogLevel:           slog.LevelInfo,
		ShutdownTimeoutSec: defaultShutdownTimeoutSec,
	}
}

//go:build ignore
// +build ignore

// Package mcp implements a Model Context Protocol server exposing Codefang
// analysis capabilities as MCP tools over stdio transport.
package mcp

import (
	"context"
	"fmt"
	"log/slog"
	"sort"
	"sync"
	"time"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

const (
	// serverName is the MCP server implementation name.
	serverName = "codefang"
	// serverVersion is the MCP server implementation version.
	serverVersion = "1.0.0"

	// toolCount is the expected number of registered tools.
	toolCount = 3
)

// ServerDeps holds injectable dependencies for the MCP server.
// Zero-value fields use production defaults.
type ServerDeps struct {
	// Logger is an optional structured logger. Nil uses slog default.
	Logger *slog.Logger

	// Metrics is an optional RED metrics recorder. Nil disables per-tool metrics.
	Metrics *observability.REDMetrics

	// Tracer is an optional OTel tracer for per-tool-call spans. Nil disables tracing.
	Tracer trace.Tracer
}

// Server wraps the MCP SDK server with Codefang tool registrations.
type Server struct {
	inner   *mcpsdk.Server
	mu      sync.RWMutex
	tools   []string
	metrics *observability.REDMetrics
	tracer  trace.Tracer
}

// NewServer creates a new MCP server with all Codefang tools registered.
func NewServer(deps ServerDeps) *Server {
	opts := &mcpsdk.ServerOptions{}
	if deps.Logger != nil {
		opts.Logger = deps.Logger
	}

	inner := mcpsdk.NewServer(
		&mcpsdk.Implementation{
			Name:    serverName,
			Version: serverVersion,
		},
		opts,
	)

	srv := &Server{
		inner:   inner,
		tools:   make([]string, 0, toolCount),
		metrics: deps.Metrics,
		tracer:  deps.Tracer,
	}

	srv.registerTools()

	return srv
}

// ListToolNames returns the sorted names of all registered tools.
func (s *Server) ListToolNames() []string {
	s.mu.RLock()
	defer s.mu.RUnlock()

	names := make([]string, len(s.tools))
	copy(names, s.tools)
	sort.Strings(names)

	return names
}

// Run starts the MCP server on stdio transport. It blocks until the context
// is canceled or the connection closes.
func (s *Server) Run(ctx context.Context) error {
	err := s.inner.Run(ctx, &mcpsdk.StdioTransport{})
	if err != nil {
		return fmt.Errorf("mcp server: %w", err)
	}

	return nil
}

// RunWithTransport starts the MCP server on the given transport. It blocks
// until the context is canceled or the connection closes.
func (s *Server) RunWithTransport(ctx context.Context, transport mcpsdk.Transport) error {
	err := s.inner.Run(ctx, transport)
	if err != nil {
		return fmt.Errorf("mcp server: %w", err)
	}

	return nil
}

// registerTools adds all Codefang MCP tools to the server.
func (s *Server) registerTools() {
	s.registerAnalyzeTool()
	s.registerUASTTool()
	s.registerHistoryTool()
}

func (s *Server) registerAnalyzeTool() {
	mcpsdk.AddTool(s.inner, &mcpsdk.Tool{
		Name:        ToolNameAnalyze,
		Description: analyzeToolDescription,
	}, withMetrics(s.metrics, ToolNameAnalyze, withTracing(s.tracer, ToolNameAnalyze, handleAnalyze)))

	s.trackTool(ToolNameAnalyze)
}

func (s *Server) registerUASTTool() {
	mcpsdk.AddTool(s.inner, &mcpsdk.Tool{
		Name:        ToolNameUAST,
		Description: uastToolDescription,
	}, withMetrics(s.metrics, ToolNameUAST, withTracing(s.tracer, ToolNameUAST, handleUASTParse)))

	s.trackTool(ToolNameUAST)
}

func (s *Server) registerHistoryTool() {
	mcpsdk.AddTool(s.inner, &mcpsdk.Tool{
		Name:        ToolNameHistory,
		Description: historyToolDescription,
	}, withMetrics(s.metrics, ToolNameHistory, withTracing(s.tracer, ToolNameHistory, handleHistory)))

	s.trackTool(ToolNameHistory)
}

// mcpSpanPrefix is the prefix for MCP tool span names.
const mcpSpanPrefix = "mcp."

// traceIDMetaKey is the metadata key for trace_id in MCP tool responses.
const traceIDMetaKey = "trace_id"

// withTracing wraps an MCP tool handler to create an OTel span per invocation
// and include trace_id in the response content when sampled.
func withTracing[Input any](
	tracer trace.Tracer,
	toolName string,
	handler func(context.Context, *mcpsdk.CallToolRequest, Input) (*mcpsdk.CallToolResult, ToolOutput, error),
) func(context.Context, *mcpsdk.CallToolRequest, Input) (*mcpsdk.CallToolResult, ToolOutput, error) {
	if tracer == nil {
		return handler
	}

	return func(ctx context.Context, req *mcpsdk.CallToolRequest, input Input) (*mcpsdk.CallToolResult, ToolOutput, error) {
		ctx, span := tracer.Start(ctx, mcpSpanPrefix+toolName,
			trace.WithSpanKind(trace.SpanKindServer),
			trace.WithAttributes(attribute.String("mcp.tool", toolName)),
		)
		defer span.End()

		result, output, err := handler(ctx, req, input)

		// Include trace_id in response when span is sampled.
		sc := span.SpanContext()
		if sc.IsSampled() && result != nil {
			traceContent := &mcpsdk.TextContent{Text: fmt.Sprintf("%s=%s", traceIDMetaKey, sc.TraceID().String())}
			result.Content = append(result.Content, traceContent)
		}

		return result, output, err
	}
}

// withMetrics wraps an MCP tool handler to record RED metrics per invocation.
func withMetrics[Input any](
	metrics *observability.REDMetrics,
	toolName string,
	handler func(context.Context, *mcpsdk.CallToolRequest, Input) (*mcpsdk.CallToolResult, ToolOutput, error),
) func(context.Context, *mcpsdk.CallToolRequest, Input) (*mcpsdk.CallToolResult, ToolOutput, error) {
	if metrics == nil {
		return handler
	}

	return func(ctx context.Context, req *mcpsdk.CallToolRequest, input Input) (*mcpsdk.CallToolResult, ToolOutput, error) {
		start := time.Now()

		decInflight := metrics.TrackInflight(ctx, "mcp."+toolName)
		defer decInflight()

		result, output, err := handler(ctx, req, input)

		status := "ok"
		if err != nil || (result != nil && result.IsError) {
			status = "error"
		}

		metrics.RecordRequest(ctx, "mcp."+toolName, status, time.Since(start))

		return result, output, err
	}
}

func (s *Server) trackTool(name string) {
	s.mu.Lock()
	defer s.mu.Unlock()

	s.tools = append(s.tools, name)
}

// Tool description constants.
const (
	analyzeToolDescription = "Analyze source code for quality metrics " +
		"(complexity, cohesion, halstead, comments, imports). " +
		"Accepts inline code and a language identifier."

	uastToolDescription = "Parse source code into a Universal Abstract Syntax Tree (UAST). " +
		"Returns a JSON representation of the AST structure."

	historyToolDescription = "Analyze Git repository history for trends " +
		"(burndown, couples, devs, file-history, imports, sentiment, shotness, typos). " +
		"Accepts a repository path and optional parameters."
)

//go:build ignore
// +build ignore

package mcp_test

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/Sumatoshi-tech/codefang/pkg/mcp"
)

func TestMCPServer_InMemoryTransport_ToolsList(t *testing.T) {
	t.Parallel()

	srv := mcp.NewServer(mcp.ServerDeps{})

	clientTransport, serverTransport := mcpsdk.NewInMemoryTransports()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	// Start server in background.
	serverDone := make(chan error, 1)

	go func() {
		serverDone <- srv.RunWithTransport(ctx, serverTransport)
	}()

	// Create client and connect.
	client := mcpsdk.NewClient(&mcpsdk.Implementation{
		Name:    "test-client",
		Version: "1.0.0",
	}, nil)

	session, err := client.Connect(ctx, clientTransport, nil)
	require.NoError(t, err)

	defer func() {
		_ = session.Close()
	}()

	// List tools.
	toolsResult, err := session.ListTools(ctx, nil)
	require.NoError(t, err)
	require.NotNil(t, toolsResult)

	toolNames := make([]string, 0, len(toolsResult.Tools))
	for _, tool := range toolsResult.Tools {
		toolNames = append(toolNames, tool.Name)
	}

	assert.Contains(t, toolNames, "codefang_analyze")
	assert.Contains(t, toolNames, "codefang_history")
	assert.Contains(t, toolNames, "uast_parse")
	assert.Len(t, toolNames, 3)

	// Verify each tool has an input schema.
	for _, tool := range toolsResult.Tools {
		assert.NotNil(t, tool.InputSchema, "tool %s missing input schema", tool.Name)
	}

	cancel()
	<-serverDone
}

func TestMCPServer_InMemoryTransport_CallAnalyze(t *testing.T) {
	t.Parallel()

	srv := mcp.NewServer(mcp.ServerDeps{})

	clientTransport, serverTransport := mcpsdk.NewInMemoryTransports()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	serverDone := make(chan error, 1)

	go func() {
		serverDone <- srv.RunWithTransport(ctx, serverTransport)
	}()

	client := mcpsdk.NewClient(&mcpsdk.Implementation{
		Name:    "test-client",
		Version: "1.0.0",
	}, nil)

	session, err := client.Connect(ctx, clientTransport, nil)
	require.NoError(t, err)

	defer func() {
		_ = session.Close()
	}()

	// Call codefang_analyze with valid Go code.
	result, err := session.CallTool(ctx, &mcpsdk.CallToolParams{
		Name: "codefang_analyze",
		Arguments: map[string]any{
			"code":     "package main\nfunc main() {}\n",
			"language": "go",
		},
	})
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError)
	assert.NotEmpty(t, result.Content)

	cancel()
	<-serverDone
}

func TestMCPServer_InMemoryTransport_CallUASTParse(t *testing.T) {
	t.Parallel()

	srv := mcp.NewServer(mcp.ServerDeps{})

	clientTransport, serverTransport := mcpsdk.NewInMemoryTransports()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	serverDone := make(chan error, 1)

	go func() {
		serverDone <- srv.RunWithTransport(ctx, serverTransport)
	}()

	client := mcpsdk.NewClient(&mcpsdk.Implementation{
		Name:    "test-client",
		Version: "1.0.0",
	}, nil)

	session, err := client.Connect(ctx, clientTransport, nil)
	require.NoError(t, err)

	defer func() {
		_ = session.Close()
	}()

	// Call uast_parse with valid Go code.
	result, err := session.CallTool(ctx, &mcpsdk.CallToolParams{
		Name: "uast_parse",
		Arguments: map[string]any{
			"code":     "package main\nfunc main() {}\n",
			"language": "go",
		},
	})
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError)
	assert.NotEmpty(t, result.Content)

	cancel()
	<-serverDone
}

func TestMCPServer_InMemoryTransport_CallAnalyze_Error(t *testing.T) {
	t.Parallel()

	srv := mcp.NewServer(mcp.ServerDeps{})

	clientTransport, serverTransport := mcpsdk.NewInMemoryTransports()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	serverDone := make(chan error, 1)

	go func() {
		serverDone <- srv.RunWithTransport(ctx, serverTransport)
	}()

	client := mcpsdk.NewClient(&mcpsdk.Implementation{
		Name:    "test-client",
		Version: "1.0.0",
	}, nil)

	session, err := client.Connect(ctx, clientTransport, nil)
	require.NoError(t, err)

	defer func() {
		_ = session.Close()
	}()

	// Call codefang_analyze with empty code.
	result, err := session.CallTool(ctx, &mcpsdk.CallToolParams{
		Name: "codefang_analyze",
		Arguments: map[string]any{
			"code":     "",
			"language": "go",
		},
	})
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	cancel()
	<-serverDone
}

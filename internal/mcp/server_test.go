//go:build ignore
// +build ignore

package mcp_test

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/mcp"
)

func TestNewServer_ReturnsNonNil(t *testing.T) {
	t.Parallel()

	srv := mcp.NewServer(mcp.ServerDeps{})
	require.NotNil(t, srv)
}

func TestNewServer_ToolsRegistered(t *testing.T) {
	t.Parallel()

	srv := mcp.NewServer(mcp.ServerDeps{})

	tools := srv.ListToolNames()
	assert.Len(t, tools, 3)
	assert.Contains(t, tools, "codefang_analyze")
	assert.Contains(t, tools, "codefang_history")
	assert.Contains(t, tools, "uast_parse")
}

func TestServer_Run_CancelledContext(t *testing.T) {
	t.Parallel()

	srv := mcp.NewServer(mcp.ServerDeps{})

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := srv.Run(ctx)
	require.Error(t, err)
}

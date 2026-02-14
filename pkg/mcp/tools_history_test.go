//go:build ignore
// +build ignore

package mcp

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestHandleHistory_EmptyRepoPath(t *testing.T) {
	t.Parallel()

	input := HistoryInput{
		RepoPath: "",
	}

	result, _, err := handleHistory(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "repo_path parameter is required")
}

func TestHandleHistory_RelativePath(t *testing.T) {
	t.Parallel()

	input := HistoryInput{
		RepoPath: "relative/path",
	}

	result, _, err := handleHistory(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "absolute path")
}

func TestHandleHistory_NonExistentPath(t *testing.T) {
	t.Parallel()

	input := HistoryInput{
		RepoPath: "/nonexistent/path/to/repo",
	}

	result, _, err := handleHistory(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "does not exist")
}

func TestHandleHistory_NonGitDir(t *testing.T) {
	t.Parallel()
	tmpDir := t.TempDir()

	input := HistoryInput{
		RepoPath: tmpDir,
	}

	result, _, err := handleHistory(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "not a git repository")
}

func TestHandleHistory_ValidRepo(t *testing.T) {
	t.Parallel()

	repoPath := findProjectRoot(t)
	if repoPath == "" {
		t.Skip("could not find project root git repository")
	}

	input := HistoryInput{
		RepoPath:    repoPath,
		Analyzers:   []string{"couples"},
		Limit:       5,
		FirstParent: true,
	}

	result, _, err := handleHistory(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError, "unexpected error: %v", extractText(result))
	assert.NotEmpty(t, result.Content)
}

func TestHandleHistory_WithLimit(t *testing.T) {
	t.Parallel()

	repoPath := findProjectRoot(t)
	if repoPath == "" {
		t.Skip("could not find project root git repository")
	}

	input := HistoryInput{
		RepoPath:    repoPath,
		Analyzers:   []string{"couples"},
		Limit:       3,
		FirstParent: true,
	}

	result, _, err := handleHistory(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError, "unexpected error: %v", extractText(result))
}

func TestHandleHistory_WithSince(t *testing.T) {
	t.Parallel()

	repoPath := findProjectRoot(t)
	if repoPath == "" {
		t.Skip("could not find project root git repository")
	}

	input := HistoryInput{
		RepoPath:    repoPath,
		Analyzers:   []string{"couples"},
		Limit:       5,
		Since:       "24h",
		FirstParent: true,
	}

	result, _, err := handleHistory(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError, "unexpected error: %v", extractText(result))
}

// findProjectRoot walks up from current directory to find a .git directory.
func findProjectRoot(tb testing.TB) string {
	tb.Helper()

	dir, err := os.Getwd()
	if err != nil {
		return ""
	}

	for {
		gitDir := filepath.Join(dir, ".git")

		info, statErr := os.Stat(gitDir)
		if statErr == nil && info.IsDir() {
			return dir
		}

		parent := filepath.Dir(dir)
		if parent == dir {
			return ""
		}

		dir = parent
	}
}

// extractText returns the text content from the first content item, or empty string.
func extractText(result *mcpsdk.CallToolResult) string {
	if len(result.Content) == 0 {
		return ""
	}

	tc, ok := result.Content[0].(*mcpsdk.TextContent)
	if !ok {
		return ""
	}

	return tc.Text
}

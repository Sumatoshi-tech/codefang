//go:build ignore
// +build ignore

package mcp

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestHandleUASTParse_ValidGoCode(t *testing.T) {
	t.Parallel()

	input := UASTParseInput{
		Code:     "package main\nfunc main() {}\n",
		Language: "go",
	}

	result, _, err := handleUASTParse(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError)
	assert.NotEmpty(t, result.Content)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	// UAST maps Go function_declaration to "Function" type.
	assert.Contains(t, text.Text, "Function")
	assert.Contains(t, text.Text, "Package")
}

func TestHandleUASTParse_EmptyCode(t *testing.T) {
	t.Parallel()

	input := UASTParseInput{
		Code:     "",
		Language: "go",
	}

	result, _, err := handleUASTParse(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "code parameter is required")
}

func TestHandleUASTParse_EmptyLanguage(t *testing.T) {
	t.Parallel()

	input := UASTParseInput{
		Code:     "package main",
		Language: "",
	}

	result, _, err := handleUASTParse(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "language parameter is required")
}

func TestHandleUASTParse_UnsupportedLanguage(t *testing.T) {
	t.Parallel()

	input := UASTParseInput{
		Code:     "some code",
		Language: "brainfuck",
	}

	result, _, err := handleUASTParse(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "unsupported language")
}

func TestHandleUASTParse_CodeTooLarge(t *testing.T) {
	t.Parallel()

	largeCode := make([]byte, MaxCodeInputBytes+1)
	for i := range largeCode {
		largeCode[i] = 'a'
	}

	input := UASTParseInput{
		Code:     string(largeCode),
		Language: "go",
	}

	result, _, err := handleUASTParse(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "exceeds maximum size")
}

func TestHandleUASTParse_WithQuery(t *testing.T) {
	t.Parallel()

	input := UASTParseInput{
		Code:     "package main\nfunc hello() {}\nfunc world() {}\n",
		Language: "go",
		Query:    "Function",
	}

	result, _, err := handleUASTParse(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "Function")
	assert.Contains(t, text.Text, "hello")
	assert.Contains(t, text.Text, "world")
}

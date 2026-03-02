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

const goSampleCode = `package main

import "fmt"

// greet prints a greeting message to stdout.
func greet(name string) {
	if name == "" {
		name = "world"
	}
	fmt.Println("Hello, " + name)
}

func main() {
	greet("codefang")
}
`

func TestHandleAnalyze_ValidGoCode(t *testing.T) {
	t.Parallel()

	input := AnalyzeInput{
		Code:     goSampleCode,
		Language: "go",
	}

	result, _, err := handleAnalyze(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError)
	assert.NotEmpty(t, result.Content)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "complexity")
}

func TestHandleAnalyze_EmptyCode(t *testing.T) {
	t.Parallel()

	input := AnalyzeInput{
		Code:     "",
		Language: "go",
	}

	result, _, err := handleAnalyze(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "code parameter is required")
}

func TestHandleAnalyze_EmptyLanguage(t *testing.T) {
	t.Parallel()

	input := AnalyzeInput{
		Code:     "package main",
		Language: "",
	}

	result, _, err := handleAnalyze(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "language parameter is required")
}

func TestHandleAnalyze_UnsupportedLanguage(t *testing.T) {
	t.Parallel()

	input := AnalyzeInput{
		Code:     "some code",
		Language: "brainfuck",
	}

	result, _, err := handleAnalyze(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "unsupported language")
}

func TestHandleAnalyze_CodeTooLarge(t *testing.T) {
	t.Parallel()

	largeCode := make([]byte, MaxCodeInputBytes+1)
	for i := range largeCode {
		largeCode[i] = 'a'
	}

	input := AnalyzeInput{
		Code:     string(largeCode),
		Language: "go",
	}

	result, _, err := handleAnalyze(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.True(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "exceeds maximum size")
}

func TestHandleAnalyze_SelectedAnalyzers(t *testing.T) {
	t.Parallel()

	input := AnalyzeInput{
		Code:      goSampleCode,
		Language:  "go",
		Analyzers: []string{"complexity"},
	}

	result, _, err := handleAnalyze(context.Background(), &mcpsdk.CallToolRequest{}, input)
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.False(t, result.IsError)

	text, ok := result.Content[0].(*mcpsdk.TextContent)
	require.True(t, ok)
	assert.Contains(t, text.Text, "complexity")
}

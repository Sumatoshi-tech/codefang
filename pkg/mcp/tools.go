//go:build ignore
// +build ignore

package mcp

import (
	"encoding/json"
	"errors"
	"fmt"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"
)

// Tool name constants.
const (
	ToolNameAnalyze = "codefang_analyze"
	ToolNameHistory = "codefang_history"
	ToolNameUAST    = "uast_parse"
)

// Input size limits.
const (
	// MaxCodeInputBytes is the maximum allowed size for inline code input (1 MB).
	MaxCodeInputBytes = 1 << 20
)

// Sentinel errors for tool input validation.
var (
	// ErrEmptyCode indicates the code parameter is empty.
	ErrEmptyCode = errors.New("code parameter is required and must not be empty")
	// ErrEmptyLanguage indicates the language parameter is empty.
	ErrEmptyLanguage = errors.New("language parameter is required and must not be empty")
	// ErrCodeTooLarge indicates the code input exceeds the size limit.
	ErrCodeTooLarge = errors.New("code input exceeds maximum size")
	// ErrEmptyRepoPath indicates the repo_path parameter is empty.
	ErrEmptyRepoPath = errors.New("repo_path parameter is required and must not be empty")
	// ErrRepoPathNotAbsolute indicates the repo_path is not an absolute path.
	ErrRepoPathNotAbsolute = errors.New("repo_path must be an absolute path")
	// ErrRepoNotFound indicates the repository path does not exist.
	ErrRepoNotFound = errors.New("repository path does not exist")
	// ErrNotGitRepo indicates the path is not a git repository.
	ErrNotGitRepo = errors.New("path is not a git repository")
	// ErrUnsupportedLanguage indicates the language is not supported by the parser.
	ErrUnsupportedLanguage = errors.New("unsupported language")
)

// Input types (auto-generate JSON schemas via struct tags).

// AnalyzeInput is the input schema for the codefang_analyze tool.
type AnalyzeInput struct {
	Analyzers []string `json:"analyzers,omitempty" jsonschema:"optional list of analyzer names to run (default: all)"`
	Code      string   `json:"code"                jsonschema:"source code to analyze"`
	Language  string   `json:"language"            jsonschema:"programming language (e.g. go python javascript)"`
}

// HistoryInput is the input schema for the codefang_history tool.
type HistoryInput struct {
	Analyzers   []string `json:"analyzers,omitempty"    jsonschema:"optional list of history analyzers (default: all)"`
	FirstParent bool     `json:"first_parent,omitempty" jsonschema:"follow only the first parent of merge commits"`
	Limit       int      `json:"limit,omitempty"        jsonschema:"maximum number of commits to analyze (default: 1000)"`
	RepoPath    string   `json:"repo_path"              jsonschema:"absolute path to a Git repository"`
	Since       string   `json:"since,omitempty"        jsonschema:"only analyze commits after this time (e.g. 24h or 2024-01-01)"`
}

// UASTParseInput is the input schema for the uast_parse tool.
type UASTParseInput struct {
	Code     string `json:"code"            jsonschema:"source code to parse into UAST"`
	Language string `json:"language"        jsonschema:"programming language (e.g. go python javascript)"`
	Query    string `json:"query,omitempty" jsonschema:"optional node type filter (e.g. function_declaration)"`
}

// Output type (used as structured output for generic AddTool).

// ToolOutput is a generic wrapper for tool results.
type ToolOutput struct {
	Data any `json:"data"`
}

// Result helpers.

// errorResult builds a CallToolResult with isError set.
func errorResult(err error) (*mcpsdk.CallToolResult, ToolOutput, error) {
	return &mcpsdk.CallToolResult{
		Content: []mcpsdk.Content{
			&mcpsdk.TextContent{Text: err.Error()},
		},
		IsError: true,
	}, ToolOutput{}, nil
}

// jsonResult builds a CallToolResult with JSON-encoded content.
func jsonResult(value any) (*mcpsdk.CallToolResult, ToolOutput, error) {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return errorResult(fmt.Errorf("encode result: %w", err))
	}

	return &mcpsdk.CallToolResult{
		Content: []mcpsdk.Content{
			&mcpsdk.TextContent{Text: string(data)},
		},
	}, ToolOutput{Data: value}, nil
}

// validateCodeInput checks common code input constraints.
func validateCodeInput(code, language string) error {
	if code == "" {
		return ErrEmptyCode
	}

	if language == "" {
		return ErrEmptyLanguage
	}

	if len(code) > MaxCodeInputBytes {
		return fmt.Errorf("%w: %d bytes (max %d)", ErrCodeTooLarge, len(code), MaxCodeInputBytes)
	}

	return nil
}

// syntheticFilename creates a filename from a language identifier for the parser.
func syntheticFilename(language string) string {
	return "code." + language
}

package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"testing"

	"github.com/spf13/cobra"
)

// testCase holds the test data for help and subcommand tests.
type testCase struct {
	wantOut string
	args    []string
	wantErr bool
}

func TestUASTCLI_HelpAndSubcommands(t *testing.T) {
	t.Parallel()

	tests := getHelpAndSubcommandTests()

	for _, currentTest := range tests {
		runHelpAndSubcommandTest(t, currentTest)
	}
}

func getHelpAndSubcommandTests() []testCase {
	return []testCase{
		{wantOut: "Unified AST CLI", args: []string{"--help"}},
		{wantOut: "Parse source code", args: []string{"parse", "--help"}},
		{wantOut: "Query parsed UAST nodes using the functional DSL.", args: []string{"query", "--help"}},
		{wantOut: "Compare two files and detect structural changes in their UAST.", args: []string{"diff", "--help"}},
		{wantOut: "unknown command", args: []string{"unknown"}, wantErr: true},
	}
}

func runHelpAndSubcommandTest(t *testing.T, currentTest testCase) {
	t.Helper()

	rootCmd := buildTestRootCmd()
	buf := new(bytes.Buffer)
	rootCmd.SetOut(buf)
	rootCmd.SetErr(buf)
	rootCmd.SetArgs(currentTest.args)

	err := rootCmd.Execute()

	assertErrorState(t, currentTest.wantErr, err, currentTest.args)
	assertOutputContains(t, buf.String(), currentTest.wantOut, currentTest.args)
}

func assertErrorState(t *testing.T, wantErr bool, err error, args []string) {
	t.Helper()

	if isErrorExpectedButNotPresent(wantErr, err) {
		t.Errorf("args %v: expected error, got nil", args)
	}

	if isErrorUnexpected(wantErr, err) {
		t.Errorf("args %v: unexpected error: %v", args, err)
	}
}

func isErrorExpectedButNotPresent(wantErr bool, err error) bool {
	return wantErr && err == nil
}

func isErrorUnexpected(wantErr bool, err error) bool {
	return !wantErr && err != nil
}

func assertOutputContains(t *testing.T, output, wantOut string, args []string) {
	t.Helper()

	if !outputContains(output, wantOut) {
		t.Errorf("args %v: output missing %q\ngot: %s", args, wantOut, output)
	}
}

func outputContains(output, wantOut string) bool {
	return strings.Contains(output, wantOut)
}

func TestUASTCLI_ParseCommand_GoFunction(t *testing.T) {
	t.Parallel()

	tmpfile := createTempGoFile(t, `package main
func add(a, b int) int { return a + b }`)
	defer os.Remove(tmpfile)

	output := runParseCommand(t, tmpfile)
	assertOutputNotEmpty(t, output)

	parsedNode := unmarshalJSONToMap(t, output)
	assertIdentifierNodeWithTokenExists(t, parsedNode, "add")
}

func createTempGoFile(t *testing.T, source string) string {
	t.Helper()

	tmpfile, err := os.CreateTemp(t.TempDir(), "test-*.go")
	if err != nil {
		t.Fatalf("failed to create temp file: %v", err)
	}

	_, writeErr := tmpfile.WriteString(source)
	if writeErr != nil {
		t.Fatalf("failed to write temp file: %v", writeErr)
	}

	tmpfile.Close()

	return tmpfile.Name()
}

func runParseCommand(t *testing.T, filename string) string {
	t.Helper()

	rootCmd := buildTestRootCmd()
	buf := new(bytes.Buffer)
	rootCmd.SetOut(buf)
	rootCmd.SetErr(buf)
	rootCmd.SetArgs([]string{"parse", filename})

	err := rootCmd.Execute()
	if err != nil {
		t.Fatalf("parse command failed: %v", err)
	}

	return strings.TrimSpace(buf.String())
}

func assertOutputNotEmpty(t *testing.T, output string) {
	t.Helper()

	if output == "" {
		t.Fatalf("no output from parse command")
	}
}

func unmarshalJSONToMap(t *testing.T, output string) map[string]any {
	t.Helper()

	var parsedNode map[string]any

	err := json.Unmarshal([]byte(output), &parsedNode)
	if err != nil {
		t.Fatalf("output is not valid JSON: %v\n%s", err, output)
	}

	return parsedNode
}

func assertIdentifierNodeWithTokenExists(t *testing.T, parsedNode map[string]any, token string) {
	t.Helper()

	if !identifierNodeWithTokenExists(parsedNode, token) {
		t.Fatalf("No identifier node with token '%s' found; got children: %+v", token, parsedNode["children"])
	}
}

//nolint:gocognit // recursive tree search is inherently complex
func identifierNodeWithTokenExists(parsedNode map[string]any, token string) bool {
	found := false

	var search func(n map[string]any)

	search = func(n map[string]any) {
		if found {
			return
		}

		if isIdentifierWithToken(n, token) {
			found = true

			return
		}

		if children, hasChildren := n["children"].([]any); hasChildren {
			for _, child := range children {
				if childNode, isMap := child.(map[string]any); isMap {
					search(childNode)
				}
			}
		}
	}

	if children, hasChildren := parsedNode["children"].([]any); hasChildren {
		for _, child := range children {
			if childNode, isMap := child.(map[string]any); isMap {
				search(childNode)
			}
		}
	}

	return found
}

func isIdentifierWithToken(nodeMap map[string]any, token string) bool {
	return nodeMap["type"] == "Identifier" && fmt.Sprintf("%v", nodeMap["token"]) == token
}

func TestUASTCLI_ParseAndQuery_RFilterMap(t *testing.T) {
	t.Parallel()

	tmpfile := createTempGoFile(t, `package main
func foo() int { return 42 }`)
	defer os.Remove(tmpfile)

	parseOutput := runParseCommand(t, tmpfile)

	tmpjson := createTempJSONFile(t, parseOutput)
	defer os.Remove(tmpjson)

	// Query for Function nodes and map their tokens -- the Go parser puts function
	// source text in the Function node's token field (e.g. "func foo() int { return 42 }").
	queryOutput := runQueryCommand(t, tmpjson, "rfilter(.type == \"Function\") |> map(.token)")
	assertOutputContainsLiteralToken(t, queryOutput, "foo")
	assertOutputContainsLiteralToken(t, queryOutput, "42")
	assertOutputDoesNotContainTreeStructure(t, queryOutput)
}

func createTempJSONFile(t *testing.T, content string) string {
	t.Helper()

	tmpjson, err := os.CreateTemp(t.TempDir(), "test-*.json")
	if err != nil {
		t.Fatalf("failed to create temp json file: %v", err)
	}

	_, writeErr := tmpjson.WriteString(content)
	if writeErr != nil {
		t.Fatalf("failed to write temp json file: %v", writeErr)
	}

	tmpjson.Close()

	return tmpjson.Name()
}

func runQueryCommand(t *testing.T, filename, query string) string {
	t.Helper()

	rootCmd := buildTestRootCmd()
	buf := new(bytes.Buffer)
	rootCmd.SetOut(buf)
	rootCmd.SetErr(buf)
	rootCmd.SetArgs([]string{"query", query, filename})

	err := rootCmd.Execute()
	if err != nil {
		t.Fatalf("query command failed: %v", err)
	}

	return buf.String()
}

func assertOutputContainsLiteralToken(t *testing.T, output, token string) {
	t.Helper()

	if !outputContains(output, token) {
		t.Errorf("expected literal token '%s' in output, got: %s", token, output)
	}
}

func assertOutputDoesNotContainTreeStructure(t *testing.T, output string) {
	t.Helper()

	if outputContainsTreeStructure(output) {
		t.Errorf("unexpected original tree structure in output: %s", output)
	}
}

func outputContainsTreeStructure(output string) bool {
	return strings.Contains(output, "Function") || strings.Contains(output, "package")
}

func TestUASTCLI_ParseAndQuery_FunctionNames_ManyFunctions(t *testing.T) {
	t.Parallel()

	largeSource := generateLargeGoSourceWithManyFunctions(30)

	tmpfile := createTempGoFile(t, largeSource)
	defer os.Remove(tmpfile)

	parseOutput := runParseCommand(t, tmpfile)

	tmpjson := createTempJSONFile(t, parseOutput)
	defer os.Remove(tmpjson)

	// Query Function nodes and map their tokens -- each Function node's token contains
	// the full function source text (e.g. "func Func0() int { return 0 }") which
	// includes the function name.
	query := "rfilter(.type == \"Function\") |> map(.token)"
	queryOutput := runQueryCommand(t, tmpjson, query)

	assertFunctionNamesPresent(t, queryOutput, 30)
}

func generateLargeGoSourceWithManyFunctions(count int) string {
	var builder strings.Builder

	builder.WriteString("package main\n\n")

	for idx := range count {
		fmt.Fprintf(&builder, "func Func%d() int { return %d }\n", idx, idx)
	}

	builder.WriteString("\nfunc main() {\n")

	for idx := range count {
		fmt.Fprintf(&builder, "\t_ = Func%d()\n", idx)
	}

	builder.WriteString("}\n")

	return builder.String()
}

func assertFunctionNamesPresent(t *testing.T, output string, count int) {
	t.Helper()

	for idx := range count {
		name := fmt.Sprintf("Func%d", idx)

		if !outputContains(output, name) {
			t.Errorf("expected function name '%s' in output, got: %s", name, output)
		}
	}
}

func buildTestRootCmd() *cobra.Command {
	rootCmd := &cobra.Command{
		Use:   "uast",
		Short: "Unified AST CLI: parse, query, format, and diff code using UAST",
	}

	rootCmd.AddCommand(parseCmd())
	rootCmd.AddCommand(queryCmd())
	rootCmd.AddCommand(diffCmd())

	return rootCmd
}

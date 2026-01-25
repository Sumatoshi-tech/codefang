package main

import (
	"bytes"
	"fmt"
	"io/ioutil"
	"os"
	"strings"
	"testing"

	"encoding/json"

	"github.com/spf13/cobra"
)

func TestUASTCLI_HelpAndSubcommands(t *testing.T) {
	tests := getHelpAndSubcommandTests()
	for _, tt := range tests {
		runHelpAndSubcommandTest(t, tt)
	}
}

func getHelpAndSubcommandTests() []struct {
	args    []string
	wantOut string
	wantErr bool
} {
	return []struct {
		args    []string
		wantOut string
		wantErr bool
	}{
		{[]string{"--help"}, "Unified AST CLI", false},
		{[]string{"parse", "--help"}, "Parse source code", false},
		{[]string{"query", "--help"}, "Query parsed UAST nodes using the functional DSL.", false},
		{[]string{"diff", "--help"}, "Compare two files and detect structural changes in their UAST.", false},
		{[]string{"unknown"}, "unknown command", true},
	}
}

func runHelpAndSubcommandTest(t *testing.T, tt struct {
	args    []string
	wantOut string
	wantErr bool
}) {
	rootCmd := buildTestRootCmd()
	buf := new(bytes.Buffer)
	rootCmd.SetOut(buf)
	rootCmd.SetErr(buf)
	rootCmd.SetArgs(tt.args)
	err := rootCmd.Execute()
	assertErrorState(t, tt.wantErr, err, tt.args)
	assertOutputContains(t, buf.String(), tt.wantOut, tt.args)
}

func assertErrorState(t *testing.T, wantErr bool, err error, args []string) {
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
	if !outputContains(output, wantOut) {
		t.Errorf("args %v: output missing %q\ngot: %s", args, wantOut, output)
	}
}

func outputContains(output, wantOut string) bool {
	return strings.Contains(output, wantOut)
}

func TestUASTCLI_ParseCommand_GoFunction(t *testing.T) {
	tmpfile := createTempGoFile(t, `package main
func add(a, b int) int { return a + b }`)
	defer os.Remove(tmpfile)
	output := runParseCommand(t, tmpfile)
	assertOutputNotEmpty(t, output)
	node := unmarshalJSONToMap(t, output)
	assertIdentifierNodeWithTokenExists(t, node, "add")
}

func createTempGoFile(t *testing.T, source string) string {
	tmpfile, err := ioutil.TempFile("", "test-*.go")
	if err != nil {
		t.Fatalf("failed to create temp file: %v", err)
	}
	if _, err := tmpfile.Write([]byte(source)); err != nil {
		t.Fatalf("failed to write temp file: %v", err)
	}
	tmpfile.Close()
	return tmpfile.Name()
}

func runParseCommand(t *testing.T, filename string) string {
	rootCmd := buildTestRootCmd()
	buf := new(bytes.Buffer)
	rootCmd.SetOut(buf)
	rootCmd.SetErr(buf)
	rootCmd.SetArgs([]string{"parse", filename})
	if err := rootCmd.Execute(); err != nil {
		t.Fatalf("parse command failed: %v", err)
	}
	return strings.TrimSpace(buf.String())
}

func assertOutputNotEmpty(t *testing.T, output string) {
	if isEmpty(output) {
		t.Fatalf("no output from parse command")
	}
}

func isEmpty(s string) bool {
	return len(s) == 0
}

func unmarshalJSONToMap(t *testing.T, output string) map[string]any {
	var node map[string]any
	if err := json.Unmarshal([]byte(output), &node); err != nil {
		t.Fatalf("output is not valid JSON: %v\n%s", err, output)
	}
	return node
}

func assertIdentifierNodeWithTokenExists(t *testing.T, node map[string]any, token string) {
	if !identifierNodeWithTokenExists(node, token) {
		t.Fatalf("No identifier node with token '%s' found; got children: %+v", token, node["children"])
	}
}

func identifierNodeWithTokenExists(node map[string]any, token string) bool {
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
		if children, ok := n["children"].([]any); ok {
			for _, c := range children {
				if cn, ok := c.(map[string]any); ok {
					search(cn)
				}
			}
		}
	}
	if children, ok := node["children"].([]any); ok {
		for _, c := range children {
			if cn, ok := c.(map[string]any); ok {
				search(cn)
			}
		}
	}
	return found
}

func isIdentifierWithToken(n map[string]any, token string) bool {
	return n["type"] == "Identifier" && fmt.Sprintf("%v", n["token"]) == token
}

func TestUASTCLI_ParseAndQuery_RFilterMap(t *testing.T) {
	tmpfile := createTempGoFile(t, `package main
func foo() int { return 42 }`)
	defer os.Remove(tmpfile)
	parseOutput := runParseCommand(t, tmpfile)
	tmpjson := createTempJSONFile(t, parseOutput)
	defer os.Remove(tmpjson)
	// Query for Function nodes and map their tokens - the Go parser puts function
	// source text in the Function node's token field (e.g. "func foo() int { return 42 }")
	queryOutput := runQueryCommand(t, tmpjson, "rfilter(.type == \"Function\") |> map(.token)")
	assertOutputContainsLiteralToken(t, queryOutput, "foo")
	assertOutputContainsLiteralToken(t, queryOutput, "42")
	assertOutputDoesNotContainTreeStructure(t, queryOutput)
}

func createTempJSONFile(t *testing.T, content string) string {
	tmpjson, err := ioutil.TempFile("", "test-*.json")
	if err != nil {
		t.Fatalf("failed to create temp json file: %v", err)
	}
	if _, err := tmpjson.Write([]byte(content)); err != nil {
		t.Fatalf("failed to write temp json file: %v", err)
	}
	tmpjson.Close()
	return tmpjson.Name()
}

func runQueryCommand(t *testing.T, filename, query string) string {
	rootCmd := buildTestRootCmd()
	buf := new(bytes.Buffer)
	rootCmd.SetOut(buf)
	rootCmd.SetErr(buf)
	rootCmd.SetArgs([]string{"query", query, filename})
	if err := rootCmd.Execute(); err != nil {
		t.Fatalf("query command failed: %v", err)
	}
	return buf.String()
}

func assertOutputContainsLiteralToken(t *testing.T, output, token string) {
	if !outputContains(output, token) {
		t.Errorf("expected literal token '%s' in output, got: %s", token, output)
	}
}

func assertOutputDoesNotContainTreeStructure(t *testing.T, output string) {
	if outputContainsTreeStructure(output) {
		t.Errorf("unexpected original tree structure in output: %s", output)
	}
}

func outputContainsTreeStructure(output string) bool {
	return strings.Contains(output, "Function") || strings.Contains(output, "package")
}

func TestUASTCLI_ParseAndQuery_FunctionNames_ManyFunctions(t *testing.T) {
	largeSource := generateLargeGoSourceWithManyFunctions(30)
	tmpfile := createTempGoFile(t, largeSource)
	defer os.Remove(tmpfile)
	parseOutput := runParseCommand(t, tmpfile)
	tmpjson := createTempJSONFile(t, parseOutput)
	defer os.Remove(tmpjson)
	// Query Function nodes and map their tokens - each Function node's token contains
	// the full function source text (e.g. "func Func0() int { return 0 }") which
	// includes the function name.
	query := "rfilter(.type == \"Function\") |> map(.token)"
	queryOutput := runQueryCommand(t, tmpjson, query)
	assertFunctionNamesPresent(t, queryOutput, 30)
}

func generateLargeGoSourceWithManyFunctions(n int) string {
	var b strings.Builder
	b.WriteString("package main\n\n")
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "func Func%d() int { return %d }\n", i, i)
	}
	b.WriteString("\nfunc main() {\n")
	for i := 0; i < n; i++ {
		fmt.Fprintf(&b, "\t_ = Func%d()\n", i)
	}
	b.WriteString("}\n")
	return b.String()
}

func assertFunctionNamesPresent(t *testing.T, output string, n int) {
	for i := 0; i < n; i++ {
		name := fmt.Sprintf("Func%d", i)
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

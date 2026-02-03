package imports //nolint:testpackage // testing internal implementation.

import (
	"bytes"
	"encoding/json"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/assert/yaml"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

func TestAnalyzer_Analyze(t *testing.T) {
	t.Parallel()

	a := &Analyzer{}
	if a.Name() == "" {
		t.Error("Name empty")
	}

	if a.Thresholds() != nil {
		t.Error("Thresholds not nil")
	}

	if a.CreateAggregator() == nil {
		t.Error("CreateAggregator is nil")
	}

	// Python: import os.
	root := &node.Node{
		Type:  node.UASTImport,
		Token: "import os",
	}

	report, err := a.Analyze(root)
	if err != nil {
		t.Fatalf("Analyze failed: %v", err)
	}

	imports, ok := report["imports"].([]string)
	require.True(t, ok, "type assertion failed for imports")

	if len(imports) != 1 {
		t.Errorf("expected 1 import, got %d", len(imports))
	}

	if imports[0] != "os" {
		t.Errorf("expected import os, got %s", imports[0])
	}
}

func TestAnalyzer_Format(t *testing.T) {
	t.Parallel()

	a := &Analyzer{}
	report := analyze.Report{
		"imports": []string{"os", "sys"},
	}

	var buf bytes.Buffer

	err := a.FormatReport(report, &buf)
	if err != nil {
		t.Fatalf("FormatReport failed: %v", err)
	}

	if !strings.Contains(buf.String(), "os") {
		t.Error("expected os in output")
	}

	var bufJSON bytes.Buffer

	err = a.FormatReportJSON(report, &bufJSON)
	if err != nil {
		t.Fatalf("FormatReportJSON failed: %v", err)
	}

	if !strings.Contains(bufJSON.String(), "os") {
		t.Error("expected os in JSON output")
	}
}

func TestExtractImportsFromUAST(t *testing.T) {
	t.Parallel()

	// 1. Python "import os".
	root1 := &node.Node{Type: node.UASTImport, Token: "import os"}

	imps1 := extractImportsFromUAST(root1)
	if len(imps1) != 1 || imps1[0] != "os" {
		t.Errorf("Python import failed: %v", imps1)
	}

	// 2. Python "from x import y".
	root2 := &node.Node{Type: node.UASTImport, Token: "from x import y"}

	imps2 := extractImportsFromUAST(root2)
	if len(imps2) != 1 || imps2[0] != "x" {
		t.Errorf("Python from import failed: %v", imps2)
	}

	// 3. JS "import React from 'react'" (Token on node?)
	// Actually parser output depends on language.
	// But `extractImportPath` handles strings.
	root3 := &node.Node{Type: node.UASTImport, Token: "import React from 'react'"}
	imps3 := extractImportsFromUAST(root3)
	// CleanImportPath splits " from " -> 'react' -> react.
	if len(imps3) != 1 || imps3[0] != "react" {
		t.Errorf("JS import failed: %v", imps3)
	}

	// 4. JS "import './styles.css'".
	root4 := &node.Node{Type: node.UASTImport, Token: "import './styles.css'"}

	imps4 := extractImportsFromUAST(root4)
	if len(imps4) != 1 || imps4[0] != "./styles.css" {
		t.Errorf("JS side-effect import failed: %v", imps4)
	}

	// 5. Children traversal (RoleImport).
	root5 := &node.Node{Roles: []node.Role{node.RoleImport}}
	child := &node.Node{Type: node.UASTLiteral, Token: "'module'"}
	root5.Children = []*node.Node{child}

	// ExtractImportPath uses Children if Token empty?
	// Logic:
	/*
		if importNode.Token != "" { return ... }
		if len(Children) > 0 { ... }
	*/
	// If Token empty, checks children.
	imps5 := extractImportsFromUAST(root5)
	if len(imps5) != 1 || imps5[0] != "module" {
		t.Errorf("Child import failed: %v", imps5)
	}
}

// --- FormatReportJSON Tests ---

func TestAnalyzer_FormatReportJSON_WithMetrics(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := analyze.Report{
		"imports": []string{"fmt", "os", "github.com/user/repo"},
		"count":   3,
	}

	var buf bytes.Buffer
	err := a.FormatReportJSON(report, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON
	var result ComputedMetrics
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure
	assert.Len(t, result.ImportList, 3)
	assert.Equal(t, 3, result.Aggregate.TotalImports)
}

func TestAnalyzer_FormatReportJSON_Empty(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := analyze.Report{}

	var buf bytes.Buffer
	err := a.FormatReportJSON(report, &buf)

	require.NoError(t, err)

	// Verify output is valid JSON
	var result ComputedMetrics
	err = json.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.ImportList)
	assert.Equal(t, 0, result.Aggregate.TotalImports)
}

// --- FormatReportYAML Tests ---

func TestAnalyzer_FormatReportYAML(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := analyze.Report{
		"imports": []string{"fmt", "os", "github.com/user/repo"},
		"count":   3,
	}

	var buf bytes.Buffer
	err := a.FormatReportYAML(report, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML
	var result ComputedMetrics
	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	// Verify metrics structure
	assert.Len(t, result.ImportList, 3)
	assert.Equal(t, 3, result.Aggregate.TotalImports)
}

func TestAnalyzer_FormatReportYAML_Empty(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := analyze.Report{}

	var buf bytes.Buffer
	err := a.FormatReportYAML(report, &buf)

	require.NoError(t, err)

	// Verify output is valid YAML
	var result ComputedMetrics
	err = yaml.Unmarshal(buf.Bytes(), &result)
	require.NoError(t, err)

	assert.Empty(t, result.ImportList)
	assert.Equal(t, 0, result.Aggregate.TotalImports)
}

func TestAnalyzer_FormatReportYAML_ContainsExpectedFields(t *testing.T) {
	t.Parallel()

	a := NewAnalyzer()
	report := analyze.Report{
		"imports": []string{"fmt"},
		"count":   1,
	}

	var buf bytes.Buffer
	err := a.FormatReportYAML(report, &buf)

	require.NoError(t, err)

	output := buf.String()
	assert.Contains(t, output, "import_list:")
	assert.Contains(t, output, "categories:")
	assert.Contains(t, output, "dependencies:")
	assert.Contains(t, output, "aggregate:")
}

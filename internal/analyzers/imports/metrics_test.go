package imports

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Test constants to avoid magic strings/numbers.
const (
	testImportStdlib   = "fmt"
	testImportExternal = "github.com/user/repo"
	testImportRelative = "../utils"
	testImportDeepPath = "github.com/org/repo/pkg/internal/utils/helper"
	testImportDeepRel  = "../../../utils"

	floatDelta = 0.01
)

// --- ParseReportData Tests ---.

func TestParseReportData_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	assert.Empty(t, result.Imports)
	assert.Equal(t, 0, result.Count)
}

func TestParseReportData_AllFields(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"imports": []string{testImportStdlib, testImportExternal},
		"count":   5,
	}

	result, err := ParseReportData(report)

	require.NoError(t, err)
	require.Len(t, result.Imports, 2)
	assert.Equal(t, testImportStdlib, result.Imports[0])
	assert.Equal(t, testImportExternal, result.Imports[1])
	assert.Equal(t, 5, result.Count)
}

// --- categorizeImport Tests ---.

func TestCategorizeImport(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		imp      string
		expected string
	}{
		{"relative_dot", "./utils", "relative"},
		{"relative_dotdot", "../utils", "relative"},
		{"relative_absolute", "/absolute/path", "relative"},
		{"stdlib_simple", "fmt", "stdlib"},
		{"stdlib_with_path", "encoding/json", "stdlib"},
		{"external_github", "github.com/user/repo", "external"},
		{"external_no_slash", "somepackage", "external"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := categorizeImport(tt.imp)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// --- isExternalImport Tests ---.

func TestIsExternalImport(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		imp      string
		expected bool
	}{
		{"relative_false", "./utils", false},
		{"relative_dotdot_false", "../utils", false},
		{"stdlib_false", "fmt", false},
		{"external_true", "github.com/user/repo", true},
		{"unknown_external", "somepackage", true},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := isExternalImport(tt.imp)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// --- isStandardLibrary Tests ---.

func TestIsStandardLibrary(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		imp      string
		expected bool
	}{
		// Go stdlib.
		{"go_fmt", "fmt", true},
		{"go_os", "os", true},
		{"go_io", "io", true},
		{"go_net", "net", true},
		{"go_http", "net/http", true},
		{"go_encoding", "encoding/json", true},
		{"go_sync", "sync", true},
		{"go_context", "context", true},
		{"go_time", "time", true},
		{"go_strings", "strings", true},
		{"go_bytes", "bytes", true},
		{"go_path", "path/filepath", true},
		{"go_regexp", "regexp", true},
		{"go_sort", "sort", true},
		{"go_math", "math/rand", true},
		// Python stdlib.
		{"py_sys", "sys", true},
		{"py_typing", "typing", true},
		{"py_collections", "collections", true},
		{"py_itertools", "itertools", true},
		{"py_functools", "functools", true},
		{"py_json", "json", true},
		{"py_re", "re", true},
		// JS/Node stdlib.
		{"node_fs", "fs", true},
		{"node_util", "util", true},
		{"node_events", "events", true},
		{"node_stream", "stream", true},
		{"node_crypto", "crypto", true},
		{"node_https", "https", true},
		// External.
		{"external", "github.com/user/repo", false},
		{"unknown", "somepackage", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := isStandardLibrary(tt.imp)
			assert.Equal(t, tt.expected, result)
		})
	}
}

// --- ImportListMetric Tests ---.

func TestNewImportListMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewImportListMetric()

	assert.Equal(t, "import_list", m.Name())
	assert.Equal(t, "Import List", m.DisplayName())
	assert.Contains(t, m.Description(), "Categorized list")
	assert.Equal(t, "list", m.Type())
}

func TestImportListMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewImportListMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestImportListMetric_SingleImport(t *testing.T) {
	t.Parallel()

	m := NewImportListMetric()
	input := &ReportData{
		Imports: []string{testImportStdlib},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, testImportStdlib, result[0].Path)
	assert.Equal(t, "stdlib", result[0].Category)
	assert.False(t, result[0].IsExternal)
}

func TestImportListMetric_MultipleImports_Sorted(t *testing.T) {
	t.Parallel()

	m := NewImportListMetric()
	input := &ReportData{
		Imports: []string{
			testImportExternal,
			testImportStdlib,
			testImportRelative,
			"os",
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 4)
	// Sorted by category then path: external < relative < stdlib.
	assert.Equal(t, "external", result[0].Category)
	assert.Equal(t, "relative", result[1].Category)
	assert.Equal(t, "stdlib", result[2].Category)
	assert.Equal(t, "stdlib", result[3].Category)
	// Within stdlib, sorted by path.
	assert.Equal(t, testImportStdlib, result[2].Path) // "fmt" < "os".
	assert.Equal(t, "os", result[3].Path)
}

// --- ImportCategoryMetric Tests ---.

func TestNewImportCategoryMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewImportCategoryMetric()

	assert.Equal(t, "import_categories", m.Name())
	assert.Equal(t, "Import Categories", m.DisplayName())
	assert.Contains(t, m.Description(), "Distribution of imports")
	assert.Equal(t, "aggregate", m.Type())
}

func TestImportCategoryMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewImportCategoryMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestImportCategoryMetric_AllCategories(t *testing.T) {
	t.Parallel()

	m := NewImportCategoryMetric()
	input := &ReportData{
		Imports: []string{
			testImportStdlib,
			"os",
			"io",
			testImportExternal,
			"github.com/other/pkg",
			testImportRelative,
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 3)
	// Sorted by count descending.
	assert.Equal(t, "stdlib", result[0].Category)
	assert.Equal(t, 3, result[0].Count)
	assert.Equal(t, "external", result[1].Category)
	assert.Equal(t, 2, result[1].Count)
	assert.Equal(t, "relative", result[2].Category)
	assert.Equal(t, 1, result[2].Count)
}

// --- ImportDependencyMetric Tests ---.

func TestNewImportDependencyMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewImportDependencyMetric()

	assert.Equal(t, "import_dependencies", m.Name())
	assert.Equal(t, "Import Dependencies", m.DisplayName())
	assert.Contains(t, m.Description(), "dependency issues")
	assert.Equal(t, "risk", m.Type())
}

func TestImportDependencyMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewImportDependencyMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestImportDependencyMetric_NoIssues(t *testing.T) {
	t.Parallel()

	m := NewImportDependencyMetric()
	input := &ReportData{
		Imports: []string{
			testImportStdlib,
			testImportExternal,
			testImportRelative, // Only 1 ..
		},
	}

	result := m.Compute(input)

	assert.Empty(t, result)
}

func TestImportDependencyMetric_DeeplyNestedRelative(t *testing.T) {
	t.Parallel()

	m := NewImportDependencyMetric()
	input := &ReportData{
		Imports: []string{
			"../../../utils",          // 3x ..
			"../../../../other/utils", // 4x ..
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 2)
	assert.Equal(t, "MEDIUM", result[0].RiskLevel)
	assert.Contains(t, result[0].Reason, "Deeply nested")
}

func TestImportDependencyMetric_LongPath(t *testing.T) {
	t.Parallel()

	m := NewImportDependencyMetric()
	input := &ReportData{
		Imports: []string{
			"github.com/org/repo/pkg/internal/utils/helper", // 6 slashes.
		},
	}

	result := m.Compute(input)

	require.Len(t, result, 1)
	assert.Equal(t, "LOW", result[0].RiskLevel)
	assert.Contains(t, result[0].Reason, "Long import path")
}

// --- ImportsAggregateMetric Tests ---.

func TestNewAggregateMetric_Metadata(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()

	assert.Equal(t, "imports_aggregate", m.Name())
	assert.Equal(t, "Imports Summary", m.DisplayName())
	assert.Contains(t, m.Description(), "Aggregate statistics")
	assert.Equal(t, "aggregate", m.Type())
}

func TestImportsAggregateMetric_Empty(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{}

	result := m.Compute(input)

	assert.Equal(t, 0, result.TotalImports)
	assert.Equal(t, 0, result.ExternalImports)
	assert.Equal(t, 0, result.InternalImports)
	assert.Equal(t, 0, result.UniquePackages)
	assert.InDelta(t, 0.0, result.ExternalRatio, floatDelta)
}

func TestImportsAggregateMetric_MixedImports(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{
		Imports: []string{
			testImportStdlib,   // internal (stdlib).
			"os",               // internal (stdlib).
			testImportExternal, // external.
			"github.com/other", // external.
			testImportRelative, // internal (relative).
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 5, result.TotalImports)
	assert.Equal(t, 2, result.ExternalImports)
	assert.Equal(t, 3, result.InternalImports)
	// Unique packages: fmt, os, github.com, ..(from relative).
	assert.GreaterOrEqual(t, result.UniquePackages, 3)
	assert.InDelta(t, 2.0/5.0, result.ExternalRatio, floatDelta)
}

func TestImportsAggregateMetric_AllExternal(t *testing.T) {
	t.Parallel()

	m := NewAggregateMetric()
	input := &ReportData{
		Imports: []string{
			testImportExternal,
			"github.com/other/pkg",
		},
	}

	result := m.Compute(input)

	assert.Equal(t, 2, result.TotalImports)
	assert.Equal(t, 2, result.ExternalImports)
	assert.Equal(t, 0, result.InternalImports)
	assert.InDelta(t, 1.0, result.ExternalRatio, floatDelta)
}

// --- ComputeAllMetrics Tests ---.

func TestComputeAllMetrics_Empty(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)
	assert.Empty(t, result.ImportList)
	assert.Empty(t, result.Categories)
	assert.Empty(t, result.Dependencies)
	assert.Equal(t, 0, result.Aggregate.TotalImports)
}

func TestComputeAllMetrics_Full(t *testing.T) {
	t.Parallel()

	report := analyze.Report{
		"imports": []string{
			testImportStdlib,
			testImportExternal,
			testImportRelative,
			"../../../deep/nested",
		},
		"count": 4,
	}

	result, err := ComputeAllMetrics(report)

	require.NoError(t, err)

	// ImportList.
	require.Len(t, result.ImportList, 4)

	// Categories.
	require.GreaterOrEqual(t, len(result.Categories), 2)

	// Dependencies - should have 1 issue (deeply nested).
	require.Len(t, result.Dependencies, 1)
	assert.Equal(t, "MEDIUM", result.Dependencies[0].RiskLevel)

	// Aggregate.
	assert.Equal(t, 4, result.Aggregate.TotalImports)
}

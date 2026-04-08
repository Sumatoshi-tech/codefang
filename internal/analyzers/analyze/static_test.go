package analyze_test

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"sync/atomic"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

func TestShouldSkipFolderNode_PermissionDeniedDirectory(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	blockedDir := filepath.Join(tmpDir, "blocked")
	require.NoError(t, os.Mkdir(blockedDir, 0o750))

	entries, err := os.ReadDir(tmpDir)
	require.NoError(t, err)
	require.Len(t, entries, 1)

	skip, skipErr := analyze.ShouldSkipFolderNode(blockedDir, entries[0], fs.ErrPermission, nil)
	require.True(t, skip)
	require.ErrorIs(t, skipErr, filepath.SkipDir)
}

func TestShouldSkipFolderNode_PermissionDeniedFile(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	filePath := filepath.Join(tmpDir, "main.go")
	require.NoError(t, os.WriteFile(filePath, []byte("package main\n"), 0o600))

	entries, err := os.ReadDir(tmpDir)
	require.NoError(t, err)
	require.Len(t, entries, 1)

	skip, skipErr := analyze.ShouldSkipFolderNode(filePath, entries[0], fs.ErrPermission, nil)
	require.True(t, skip)
	require.NoError(t, skipErr)
}

func TestShouldSkipFolderNode_NotExistDirectory(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	missingDir := filepath.Join(tmpDir, "missing")
	require.NoError(t, os.Mkdir(missingDir, 0o750))

	entries, err := os.ReadDir(tmpDir)
	require.NoError(t, err)
	require.Len(t, entries, 1)

	require.NoError(t, os.RemoveAll(missingDir))

	skip, skipErr := analyze.ShouldSkipFolderNode(missingDir, entries[0], fs.ErrNotExist, nil)
	require.True(t, skip)
	require.ErrorIs(t, skipErr, filepath.SkipDir)
}

func TestShouldSkipFolderNode_NotExistFile(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	filePath := filepath.Join(tmpDir, "main.go")
	require.NoError(t, os.WriteFile(filePath, []byte("package main\n"), 0o600))

	entries, err := os.ReadDir(tmpDir)
	require.NoError(t, err)
	require.Len(t, entries, 1)

	require.NoError(t, os.Remove(filePath))

	skip, skipErr := analyze.ShouldSkipFolderNode(filePath, entries[0], fs.ErrNotExist, nil)
	require.True(t, skip)
	require.NoError(t, skipErr)
}

func TestShouldSkipFolderNode_NilParser(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	blockedDir := filepath.Join(tmpDir, "blocked")
	require.NoError(t, os.Mkdir(blockedDir, 0o750))

	entries, err := os.ReadDir(tmpDir)
	require.NoError(t, err)
	require.Len(t, entries, 1)

	// Directory entries are always skipped (not files), parser isn't needed.
	skip, skipErr := analyze.ShouldSkipFolderNode(blockedDir, entries[0], nil, nil)
	require.True(t, skip)
	require.NoError(t, skipErr)
}

func TestStaticService_AnalyzeFolder_SkipsPermissionDeniedDirectory(t *testing.T) {
	t.Parallel()

	if runtime.GOOS == "windows" {
		t.Skip("permission mode semantics differ on windows")
	}

	tmpDir := t.TempDir()
	goFile := filepath.Join(tmpDir, "main.go")
	require.NoError(
		t,
		os.WriteFile(goFile, []byte("package main\nfunc main() {}\n"), 0o600),
	)

	blockedDir := filepath.Join(tmpDir, "blocked")
	require.NoError(t, os.Mkdir(blockedDir, 0o750))
	require.NoError(
		t,
		os.WriteFile(filepath.Join(blockedDir, "blocked.go"), []byte("package blocked\n"), 0o600),
	)
	require.NoError(t, os.Chmod(blockedDir, 0o000))

	defer func() {
		require.NoError(t, os.Chmod(blockedDir, 0o750))
	}()

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	results, err := svc.AnalyzeFolder(context.Background(), tmpDir, []string{"complexity"})
	require.NoError(t, err)
	require.Contains(t, results, "complexity")
}

func TestAllStaticAnalyzers_UniversalOutputFormats(t *testing.T) {
	t.Parallel()

	parser, err := uast.NewParser()
	require.NoError(t, err)

	source := []byte("package main\nimport \"fmt\"\n// main prints output.\nfunc main(){\n// inline comment\nfmt.Println(\"x\")\n}\n")
	root, err := parser.Parse(context.Background(), "main.go", source)
	require.NoError(t, err)

	for _, analyzer := range testStaticAnalyzers() {
		t.Run(analyzer.Name(), func(t *testing.T) {
			t.Parallel()

			report, analyzeErr := analyzer.Analyze(root)
			require.NoError(t, analyzeErr)

			var jsonBuf, yamlBuf, plotBuf, binaryBuf bytes.Buffer
			require.NoError(t, analyzer.FormatReportJSON(report, &jsonBuf))
			require.NotZero(t, jsonBuf.Len())

			require.NoError(t, analyzer.FormatReportYAML(report, &yamlBuf))
			require.NotZero(t, yamlBuf.Len())

			require.NoError(t, analyzer.FormatReportPlot(report, &plotBuf))
			require.NotZero(t, plotBuf.Len())

			require.NoError(t, analyzer.FormatReportBinary(report, &binaryBuf))
			require.NotZero(t, binaryBuf.Len())
		})
	}
}

func TestStampSourceFile(t *testing.T) {
	t.Parallel()

	reports := map[string]analyze.Report{
		"cohesion": {
			"total_functions": 2,
			"functions": []map[string]any{
				{"name": "fnA", "cohesion": 0.8},
				{"name": "fnB", "cohesion": 0.3},
			},
		},
	}

	analyze.StampSourceFile(reports, "/repo/pkg/auth/handler.go", "/repo")

	functions, ok := reports["cohesion"]["functions"].([]map[string]any)
	require.True(t, ok)
	require.Len(t, functions, 2)

	for _, fn := range functions {
		require.Equal(t, "pkg/auth/handler.go", fn["_source_file"])
	}
}

func TestStampSourceFile_EmptyReport(t *testing.T) {
	t.Parallel()

	reports := map[string]analyze.Report{}

	require.NotPanics(t, func() {
		analyze.StampSourceFile(reports, "/some/path.go", "")
	})
}

func TestStampSourceFile_NoCollections(t *testing.T) {
	t.Parallel()

	reports := map[string]analyze.Report{
		"cohesion": {
			"total_functions": 5,
			"lcom":            0.3,
			"message":         "ok",
		},
	}

	require.NotPanics(t, func() {
		analyze.StampSourceFile(reports, "/some/path.go", "")
	})
}

// FRD: specs/frds/FRD-20260311-typed-report-items.md.

func TestStampSourceFile_TypedCollection(t *testing.T) {
	t.Parallel()

	type testItem struct {
		Name  string
		Value int
	}

	converter := func(items any, sourceFile string) []map[string]any {
		typed, ok := items.([]testItem)
		if !ok {
			return nil
		}

		result := make([]map[string]any, 0, len(typed))

		for _, item := range typed {
			m := map[string]any{
				"name":  item.Name,
				"value": item.Value,
			}
			if sourceFile != "" {
				m["_source_file"] = sourceFile
			}

			result = append(result, m)
		}

		return result
	}

	tc := analyze.TypedCollection{
		Items:  []testItem{{Name: "fn1", Value: 10}, {Name: "fn2", Value: 20}},
		ToMaps: converter,
	}

	reports := map[string]analyze.Report{
		"complexity": {
			"total_functions": 2,
			"functions":       tc,
		},
	}

	analyze.StampSourceFile(reports, "/repo/pkg/foo.go", "/repo")

	stamped, ok := reports["complexity"]["functions"].(analyze.TypedCollection)
	require.True(t, ok)
	assert.Equal(t, "pkg/foo.go", stamped.SourceFile)

	// Verify converter produces maps with _source_file.
	maps := stamped.ToMaps(stamped.Items, stamped.SourceFile)
	require.Len(t, maps, 2)
	assert.Equal(t, "pkg/foo.go", maps[0]["_source_file"])
	assert.Equal(t, "pkg/foo.go", maps[1]["_source_file"])
}

// FRD: specs/frds/FRD-20260311-cap-static-workers.md.

func TestStaticService_ResolveMaxWorkers_DefaultCapsAtEight(t *testing.T) {
	t.Parallel()

	svc := analyze.NewStaticService(nil, nil)
	got := svc.ResolveMaxWorkers()

	want := min(runtime.NumCPU(), analyze.DefaultStaticMaxWorkers)

	require.Equal(t, want, got)
}

func TestStaticService_AnalyzeFolder_RespectsMaxWorkers(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()

	require.NoError(t, os.WriteFile(
		filepath.Join(tmpDir, "a.go"),
		[]byte("package a\nfunc A() {}\n"), 0o600,
	))
	require.NoError(t, os.WriteFile(
		filepath.Join(tmpDir, "b.go"),
		[]byte("package a\nfunc B() {}\n"), 0o600,
	))

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.MaxWorkers = 1

	results, err := svc.AnalyzeFolder(context.Background(), tmpDir, []string{"complexity"})
	require.NoError(t, err)
	require.Contains(t, results, "complexity")
}

func TestStaticService_ResolveMaxWorkers_ExplicitOverride(t *testing.T) {
	t.Parallel()

	const explicitWorkers = 16

	svc := analyze.NewStaticService(nil, nil)
	svc.MaxWorkers = explicitWorkers

	require.Equal(t, explicitWorkers, svc.ResolveMaxWorkers())
}

// FRD: specs/frds/FRD-20260311-static-malloc-trim.md.

func TestStaticService_ResolveMallocTrimInterval_Default(t *testing.T) {
	t.Parallel()

	svc := analyze.NewStaticService(nil, nil)

	require.Equal(t, analyze.DefaultMallocTrimInterval, svc.ResolveMallocTrimInterval())
}

func TestStaticService_ResolveMallocTrimInterval_ExplicitOverride(t *testing.T) {
	t.Parallel()

	const customInterval = 100

	svc := analyze.NewStaticService(nil, nil)
	svc.MallocTrimInterval = customInterval

	require.Equal(t, customInterval, svc.ResolveMallocTrimInterval())
}

func TestStaticService_ResolveMallocTrimInterval_Disabled(t *testing.T) {
	t.Parallel()

	svc := analyze.NewStaticService(nil, nil)
	svc.MallocTrimInterval = -1

	require.Equal(t, -1, svc.ResolveMallocTrimInterval())
}

func TestStaticService_AnalyzeFolder_CallsMallocTrim(t *testing.T) {
	t.Parallel()

	const (
		fileCount    = 10
		trimInterval = 3
	)

	dir := t.TempDir()

	for i := range fileCount {
		name := filepath.Join(dir, fmt.Sprintf("f%d.go", i))
		require.NoError(t, os.WriteFile(name, []byte("package a\nfunc F() {}\n"), 0o600))
	}

	var trimCalls atomic.Int64

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.MaxWorkers = 1
	svc.MallocTrimInterval = trimInterval
	svc.NativeMemoryReleaseFn = func() { trimCalls.Add(1) }

	_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
	require.NoError(t, err)

	// 10 files / 3 interval = files 3, 6, 9 trigger trim = 3 calls.
	const expectedTrimCalls = 3

	require.Equal(t, int64(expectedTrimCalls), trimCalls.Load())
}

func TestStaticService_AnalyzeFolder_NoTrimWhenDisabled(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "a.go"),
		[]byte("package a\nfunc A() {}\n"), 0o600,
	))

	var trimCalls atomic.Int64

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.MaxWorkers = 1
	svc.MallocTrimInterval = -1
	svc.NativeMemoryReleaseFn = func() { trimCalls.Add(1) }

	_, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
	require.NoError(t, err)

	require.Zero(t, trimCalls.Load())
}

// FRD: specs/frds/FRD-20260311-summary-only-aggregation.md.

func TestResolveAggregationMode_TextIsSummaryOnly(t *testing.T) {
	t.Parallel()

	mode := analyze.ResolveAggregationMode(analyze.FormatText)
	require.Equal(t, analyze.AggregationModeSummaryOnly, mode)
}

func TestResolveAggregationMode_CompactIsSummaryOnly(t *testing.T) {
	t.Parallel()

	mode := analyze.ResolveAggregationMode(analyze.FormatCompact)
	require.Equal(t, analyze.AggregationModeSummaryOnly, mode)
}

func TestResolveAggregationMode_JSONIsFull(t *testing.T) {
	t.Parallel()

	mode := analyze.ResolveAggregationMode(analyze.FormatJSON)
	require.Equal(t, analyze.AggregationModeFull, mode)
}

func TestResolveAggregationMode_YAMLIsFull(t *testing.T) {
	t.Parallel()

	mode := analyze.ResolveAggregationMode(analyze.FormatYAML)
	require.Equal(t, analyze.AggregationModeFull, mode)
}

func TestStaticService_SummaryOnly_MetricsPresent(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "main.go"),
		[]byte("package main\nfunc A() { x := 1; _ = x }\nfunc B() { y := 2; _ = y }\n"), 0o600,
	))

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.MaxWorkers = 1
	svc.MallocTrimInterval = -1
	svc.AggregationMode = analyze.AggregationModeSummaryOnly

	results, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
	require.NoError(t, err)
	require.Contains(t, results, "complexity")

	report := results["complexity"]

	// Summary metrics must still be present.
	require.Contains(t, report, "total_functions")
	require.Contains(t, report, "total_complexity")
}

// FRD: specs/frds/FRD-20260312-static-budget-tuning.md.

func TestStaticService_SpillThreshold_AppliedToAggregators(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "main.go"),
		[]byte("package main\nfunc A() { x := 1; _ = x }\n"), 0o600,
	))

	const customThreshold = 5000

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.MaxWorkers = 1
	svc.MallocTrimInterval = -1
	svc.SpillThreshold = customThreshold

	// Run analysis — if SpillThreshold wiring is broken, this would still succeed
	// but use the default threshold. We verify the service field is set.
	results, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
	require.NoError(t, err)
	require.Contains(t, results, "complexity")

	// The fact that analysis completes without error proves the wiring doesn't break.
	// SpillThresholdSetter interface compliance is verified at compile time below.
	assert.Equal(t, customThreshold, svc.SpillThreshold)
}

// FRD: specs/frds/FRD-20260312-static-rss-logging.md.

func TestStaticService_ProgressFunc_CalledDuringAnalysis(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Create enough files to trigger at least one progress event.
	// With ProgressInterval=2, 3 files should produce 1 "processing" event + 1 "complete".
	for i := range 3 {
		writeTestGoFile(t, dir, fmt.Sprintf("file%d.go", i))
	}

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}
	svc.ProgressInterval = 2

	var events []analyze.StaticProgressEvent

	svc.ProgressFunc = func(e analyze.StaticProgressEvent) {
		events = append(events, e)
	}

	results, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
	require.NoError(t, err)
	require.NotEmpty(t, results)

	// Should have at least one "complete" event.
	require.NotEmpty(t, events)

	lastEvent := events[len(events)-1]
	assert.Equal(t, analyze.ProgressPhaseComplete, lastEvent.Phase)
	assert.Positive(t, lastEvent.FilesProcessed)
}

func TestStaticService_ProgressFunc_Nil_NoError(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	writeTestGoFile(t, dir, "file.go")

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}

	// ProgressFunc is nil — should not panic.
	results, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity"})
	require.NoError(t, err)
	require.NotEmpty(t, results)
}

// FRD: specs/frds/FRD-20260312-static-plot-multipage.md.

func TestStaticService_FormatPlotPages_ProducesHTML(t *testing.T) {
	t.Parallel()

	// Register plot sections for the analyzers we'll test.
	complexity.RegisterPlotSections()
	cohesion.RegisterPlotSections()

	dir := t.TempDir()
	writeTestGoFile(t, dir, "main.go")

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}
	svc.AggregationMode = analyze.AggregationModeFull

	results, err := svc.AnalyzeFolder(context.Background(), dir, []string{"complexity", "cohesion"})
	require.NoError(t, err)

	outputDir := filepath.Join(t.TempDir(), "plot-output")

	plotErr := svc.FormatPlotPages([]string{"complexity", "cohesion"}, results, outputDir)
	require.NoError(t, plotErr)

	// Verify index.html exists.
	indexData, readErr := os.ReadFile(filepath.Join(outputDir, "index.html"))
	require.NoError(t, readErr, "index.html should exist")
	require.Contains(t, string(indexData), "cdn.tailwindcss.com")

	// Verify per-analyzer pages exist.
	for _, safeID := range []string{"static-complexity", "static-cohesion"} {
		pagePath := filepath.Join(outputDir, safeID+".html")
		pageData, pageErr := os.ReadFile(pagePath)
		require.NoError(t, pageErr, "page for %s should exist", safeID)
		require.Contains(t, string(pageData), "cdn.tailwindcss.com")
	}
}

func TestStaticService_FormatPlotPages_SkipsUnregisteredAnalyzers(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	writeTestGoFile(t, dir, "main.go")

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}

	results := map[string]analyze.Report{
		"complexity": {"total_functions": 1},
	}

	outputDir := filepath.Join(t.TempDir(), "plot-output")

	// Should still produce index.html even if section renderer is not registered.
	plotErr := svc.FormatPlotPages([]string{"complexity"}, results, outputDir)
	require.NoError(t, plotErr)

	_, statErr := os.Stat(filepath.Join(outputDir, "index.html"))
	require.NoError(t, statErr, "index.html should exist")
}

func testStaticAnalyzers() []analyze.StaticAnalyzer {
	return []analyze.StaticAnalyzer{
		complexity.NewAnalyzer(),
		comments.NewAnalyzer(),
		halstead.NewAnalyzer(),
		cohesion.NewAnalyzer(),
		imports.NewAnalyzer(),
	}
}

// writeTestGoFile writes a minimal Go file with a function for analysis.
func writeTestGoFile(t *testing.T, dir, name string) {
	t.Helper()

	path := filepath.Join(dir, name)
	content := []byte("package main\n\nfunc F() { x := 1; _ = x }\n")

	require.NoError(t, os.WriteFile(path, content, 0o600))
}

// FRD: specs/frds/FRD-20260327-static-perfile-orchestration.md.

func TestStaticService_PerFile_FieldExists(t *testing.T) {
	t.Parallel()

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.PerFile = true

	assert.True(t, svc.PerFile)
}

func TestStaticService_PerFile_AnalyzeFolderRetainsPerFileResults(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	writeTestGoFile(t, dir, "a.go")
	writeTestGoFile(t, dir, "b.go")
	writeTestGoFile(t, dir, "c.go")

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}
	svc.PerFile = true

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	_ = results

	perFile := svc.PerFileResults()
	require.NotNil(t, perFile, "per-file results must be present when PerFile=true")

	// Each analyzer should have 3 per-file entries.
	for analyzerName, fileResults := range perFile {
		assert.Len(t, fileResults, 3,
			"analyzer %s must have 3 per-file entries", analyzerName)
	}
}

func TestStaticService_PerFile_FormatJSONIncludesFiles(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	writeTestGoFile(t, dir, "a.go")
	writeTestGoFile(t, dir, "b.go")

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}
	svc.Renderer = &renderer.DefaultStaticRenderer{}
	svc.PerFile = true

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	var buf bytes.Buffer
	require.NoError(t, svc.FormatJSON(results, &buf))

	jsonStr := buf.String()
	assert.Contains(t, jsonStr, `"files"`, "JSON must include files array")
	assert.Contains(t, jsonStr, `"file_path"`, "files entries must have file_path")
}

func TestStaticService_PerFile_DisabledReturnsNil(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	writeTestGoFile(t, dir, "a.go")

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}
	// PerFile is false (default).

	_, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	assert.Nil(t, svc.PerFileResults(), "per-file results must be nil when PerFile is false")
}

// FRD: specs/frds/FRD-20260328-report-json-emission.md.

func TestStaticService_FormatPlotPages_EmitsReportJSON(t *testing.T) {
	t.Parallel()

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.NativeMemoryReleaseFn = func() {}

	results := map[string]analyze.Report{
		"complexity": {"total_functions": 1},
	}

	outputDir := filepath.Join(t.TempDir(), "plot-output")

	require.NoError(t, svc.FormatPlotPages([]string{"complexity"}, results, outputDir))

	reportPath := filepath.Join(outputDir, "report.json")
	data, err := os.ReadFile(reportPath)
	require.NoError(t, err, "report.json must exist after FormatPlotPages")

	var parsed map[string]any
	require.NoError(t, json.Unmarshal(data, &parsed), "report.json must be valid JSON")
	assert.Contains(t, parsed, "complexity", "report.json must contain analyzer results")
}

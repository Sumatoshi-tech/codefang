package analyze_test

import (
	"bytes"
	"context"
	"io/fs"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
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

	svc := analyze.NewStaticService(testStaticAnalyzers())
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

	analyze.StampSourceFile(reports, "/repo/pkg/auth/handler.go")

	functions, ok := reports["cohesion"]["functions"].([]map[string]any)
	require.True(t, ok)
	require.Len(t, functions, 2)

	for _, fn := range functions {
		require.Equal(t, "/repo/pkg/auth/handler.go", fn["_source_file"])
	}
}

func TestStampSourceFile_EmptyReport(t *testing.T) {
	t.Parallel()

	reports := map[string]analyze.Report{}

	require.NotPanics(t, func() {
		analyze.StampSourceFile(reports, "/some/path.go")
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
		analyze.StampSourceFile(reports, "/some/path.go")
	})
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

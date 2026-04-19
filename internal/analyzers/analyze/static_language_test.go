package analyze_test

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/composition"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing/pathpolicy"
)

func TestStaticService_AnalyzeFolder_PathPolicy_DefaultsDropVendorAndGenerated(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "keep.go"),
			[]byte("package main\nfunc F() {}\n"), 0o600))
	require.NoError(t,
		os.MkdirAll(filepath.Join(tmpDir, "vendor", "lib"), 0o750))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "vendor", "lib", "vendored.go"),
			[]byte("package lib\nfunc F() {}\n"), 0o600))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "api.pb.go"),
			[]byte("package main\nfunc F() {}\n"), 0o600))

	composer := composition.NewAnalyzer()
	svc := analyze.NewStaticService(nil, []analyze.RawFileAnalyzer{composer})

	results, err := svc.AnalyzeFolder(context.Background(), tmpDir, []string{composer.Name()})
	require.NoError(t, err)

	report := results[composer.Name()]
	assert.EqualValues(t, 1, report["total_files"],
		"default path policy must drop vendor/lib/vendored.go and api.pb.go")
}

func TestStaticService_AnalyzeFolder_PathPolicy_IncludeVendoredAndGeneratedRestoresAll(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "keep.go"),
			[]byte("package main\nfunc F() {}\n"), 0o600))
	require.NoError(t,
		os.MkdirAll(filepath.Join(tmpDir, "vendor", "lib"), 0o750))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "vendor", "lib", "vendored.go"),
			[]byte("package lib\nfunc F() {}\n"), 0o600))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "api.pb.go"),
			[]byte("package main\nfunc F() {}\n"), 0o600))

	composer := composition.NewAnalyzer()
	svc := analyze.NewStaticService(nil, []analyze.RawFileAnalyzer{composer})
	svc.PathPolicy = pathpolicy.Options{
		IncludeVendored:  true,
		IncludeGenerated: true,
	}

	results, err := svc.AnalyzeFolder(context.Background(), tmpDir, []string{composer.Name()})
	require.NoError(t, err)

	report := results[composer.Name()]
	assert.EqualValues(t, 3, report["total_files"],
		"include-vendored + include-generated must restore today's default behavior")
}

func TestStaticService_AnalyzeFolder_NilLanguageGlobs_ProcessesAllSupportedFiles(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "a.go"),
			[]byte("package main\nfunc F() {}\n"), 0o600))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "b.py"),
			[]byte("def f():\n    pass\n"), 0o600))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "c.js"),
			[]byte("function f() {}\n"), 0o600))

	composer := composition.NewAnalyzer()
	svc := analyze.NewStaticService(nil, []analyze.RawFileAnalyzer{composer})

	results, err := svc.AnalyzeFolder(context.Background(), tmpDir, []string{composer.Name()})
	require.NoError(t, err)

	report := results[composer.Name()]
	assert.EqualValues(t, 3, report["total_files"],
		"nil LanguageGlobs must preserve today's behavior: all 3 files processed")
}

func TestStaticService_AnalyzeFolder_LanguageGlobs_FiltersRawFileWalk(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "keep.go"),
			[]byte("package main\nfunc F() {}\n"), 0o600))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "drop.py"),
			[]byte("def f():\n    pass\n"), 0o600))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "drop.js"),
			[]byte("function f() {}\n"), 0o600))

	composer := composition.NewAnalyzer()
	svc := analyze.NewStaticService(nil, []analyze.RawFileAnalyzer{composer})
	svc.LanguageGlobs = []string{"*.go"}

	results, err := svc.AnalyzeFolder(context.Background(), tmpDir, []string{composer.Name()})
	require.NoError(t, err)
	require.Contains(t, results, composer.Name())

	report := results[composer.Name()]

	assert.EqualValues(t, 1, report["total_files"],
		"raw-file walker must skip paths outside LanguageGlobs: "+
			"only keep.go should reach the composition analyzer")
}

func TestStaticService_AnalyzeFolder_LanguageGlobs_FiltersUASTWalk(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "keep.go"),
			[]byte("package main\nfunc F() { x := 1; _ = x }\n"), 0o600))
	require.NoError(t,
		os.WriteFile(filepath.Join(tmpDir, "drop.py"),
			[]byte("def f():\n    x = 1\n    return x\n"), 0o600))

	svc := analyze.NewStaticService(testStaticAnalyzers(), nil)
	svc.LanguageGlobs = []string{"*.go"}
	svc.PerFile = true

	results, err := svc.AnalyzeFolder(context.Background(), tmpDir, []string{"complexity"})
	require.NoError(t, err)
	require.Contains(t, results, "complexity")

	perFile := svc.PerFileResults()["complexity"]
	assert.Contains(t, perFile, "keep.go",
		"Go file must reach the complexity analyzer when pathspec *.go is active")
	assert.NotContains(t, perFile, "drop.py",
		"Python file must be filtered out before the parser runs")
}

func TestMatchesLanguageGlobs_NilGlobs_AllowsAnyName(t *testing.T) {
	t.Parallel()

	assert.True(t, analyze.LanguageGlobMatcher("anything.go", nil),
		"nil globs must be treated as no-filter and return true")
}

func TestMatchesLanguageGlobs_MultipleGlobs_MatchesUnion(t *testing.T) {
	t.Parallel()

	globs := []string{"*.go", "Dockerfile"}

	assert.True(t, analyze.LanguageGlobMatcher("main.go", globs))
	assert.True(t, analyze.LanguageGlobMatcher("Dockerfile", globs))
	assert.False(t, analyze.LanguageGlobMatcher("main.py", globs),
		"a name matching neither glob must be rejected")
}

func TestMatchesLanguageGlobs_StarDotGo_MatchesGoBasename(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name   string
		path   string
		want   bool
		reason string
	}{
		{"go file", "foo.go", true, "*.go glob must match plain .go"},
		{"nested go file", "/abs/dir/foo.go", true, "match on basename, not full path"},
		{"python file", "foo.py", false, "*.go must not match .py"},
		{"no extension", "Makefile", false, "*.go must not match extensionless"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := analyze.LanguageGlobMatcher(tt.path, []string{"*.go"})
			assert.Equal(t, tt.want, got, tt.reason)
		})
	}
}

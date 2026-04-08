//go:build e2e

// FRD: specs/frds/FRD-20260404-static-composition-analyzer.md.

package e2e_test

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/composition"
)

func newCompositionService() *analyze.StaticService {
	svc := analyze.NewStaticService(nil, []analyze.RawFileAnalyzer{composition.NewAnalyzer()})
	svc.Renderer = &renderer.DefaultStaticRenderer{}

	return svc
}

func compositionFixtureDir(t *testing.T) string {
	t.Helper()

	dir := t.TempDir()

	// Source files.
	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "main.go"),
		[]byte("package main\n\nfunc main() {}\n"),
		0o600,
	))

	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "lib.go"),
		[]byte("package main\n\nfunc helper() int { return 1 }\n"),
		0o600,
	))

	// Documentation.
	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "README.md"),
		[]byte("# Project\n"),
		0o600,
	))

	// Config.
	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "config.yml"),
		[]byte("key: value\n"),
		0o600,
	))

	// Binary file.
	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "data.bin"),
		[]byte{0x00, 0x01, 0x02, 0xFF, 0xFE, 0x00, 0x00, 0x00},
		0o600,
	))

	return dir
}

func TestComposition_AnalyzeFolder_ProducesResults(t *testing.T) {
	t.Parallel()

	svc := newCompositionService()
	dir := compositionFixtureDir(t)

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)
	require.Contains(t, results, "composition")

	report := results["composition"]

	total, ok := report["total_files"].(int)
	require.True(t, ok)

	const expectedFiles = 5

	assert.Equal(t, expectedFiles, total,
		"fixture has 5 files: 2 .go + 1 .md + 1 .yml + 1 .bin")
}

func TestComposition_JSONOutput_HasSections(t *testing.T) {
	t.Parallel()

	svc := newCompositionService()
	dir := compositionFixtureDir(t)

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	sections := svc.BuildSections(results)
	require.Len(t, sections, 1)
	assert.Equal(t, "COMPOSITION", sections[0].SectionTitle())
}

func TestComposition_JSONOutput_ValidSchema(t *testing.T) {
	t.Parallel()

	svc := newCompositionService()
	dir := compositionFixtureDir(t)

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	jsonReport := svc.Renderer.SectionsToJSON(svc.BuildSections(results))

	data, marshalErr := json.Marshal(jsonReport)
	require.NoError(t, marshalErr)

	jsonStr := string(data)
	assert.Contains(t, jsonStr, "COMPOSITION")
	assert.Contains(t, jsonStr, "Total Files")
	assert.Contains(t, jsonStr, "Source Files")
}

func TestComposition_Distribution_ContainsCategories(t *testing.T) {
	t.Parallel()

	svc := newCompositionService()
	dir := compositionFixtureDir(t)

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	sections := svc.BuildSections(results)
	require.Len(t, sections, 1)

	dist := sections[0].Distribution()
	require.NotNil(t, dist)

	labels := make([]string, 0, len(dist))
	for _, item := range dist {
		labels = append(labels, item.Label)
	}

	assert.Contains(t, labels, "source")
	assert.Contains(t, labels, "binary")
}

func TestComposition_MixedRun_WithUASTAnalyzers(t *testing.T) {
	t.Parallel()

	svc := analyze.NewStaticService(allStaticAnalyzers(), []analyze.RawFileAnalyzer{composition.NewAnalyzer()})
	svc.Renderer = &renderer.DefaultStaticRenderer{}
	svc.NativeMemoryReleaseFn = func() {}

	dir := fixtureDir(t, 3)

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	// UAST analyzers produced results.
	assert.Contains(t, results, "complexity")
	assert.Contains(t, results, "imports")

	// Content analyzer also produced results.
	assert.Contains(t, results, "composition")
}

//go:build e2e

package e2e_test

// Acceptance tests for specs/filestats/SPEC.md — Feature 1 (Per-File Output).

import (
	"context"
	"os"
	"path/filepath"
	"sort"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// ---------------------------------------------------------------------------
// Baseline: current schema (must stay green)
// ---------------------------------------------------------------------------

func TestPerFile_DefaultOutput_MatchesCurrentSchema(t *testing.T) {
	t.Parallel()

	dir := fixtureDir(t, 5)
	report := runStaticJSON(t, newStaticService(), dir)

	// Top-level keys.
	assert.Contains(t, report, "overall_score")
	assert.Contains(t, report, "overall_score_label")
	_, hasTitle := report["title"]
	assert.False(t, hasTitle, "top-level 'title' must NOT exist in JSONReport")

	// One section per analyzer.
	secs := jSections(t, report)
	want := []string{"COHESION", "COMMENTS", "COMPLEXITY", "HALSTEAD", "IMPORTS"}
	got := make([]string, 0, len(secs))
	for _, s := range secs {
		if t, ok := s["title"].(string); ok {
			got = append(got, t)
		}
	}
	sort.Strings(got)
	assert.Equal(t, want, got)

	// Each section has standard fields.
	for _, s := range secs {
		title, _ := s["title"].(string)
		for _, key := range []string{"score", "score_label", "status", "metrics", "issues"} {
			assert.Contains(t, s, key, "%s must have %q", title, key)
		}
	}
}

// ---------------------------------------------------------------------------
// Per-file output: files[] array
// ---------------------------------------------------------------------------

func TestPerFile_FilesArray(t *testing.T) {
	t.Parallel()

	const n = 5

	dir := fixtureDir(t, n)
	report := runStaticJSON(t, newPerFileStaticService(), dir)

	for _, s := range jSections(t, report) {
		title, _ := s["title"].(string)

		files := jArray(s, "files")
		if !assert.NotNil(t, files,
			"%s: section must have 'files' key with --per-file", title) {
			continue
		}

		assert.Len(t, files, n, "%s: files[] length must equal source file count", title)
	}
}

func TestPerFile_FileEntrySchema(t *testing.T) {
	t.Parallel()

	dir := fixtureDir(t, 3)
	report := runStaticJSON(t, newPerFileStaticService(), dir)

	required := []string{"file_path", "score", "score_label", "status", "metrics", "issues"}

	for _, s := range jSections(t, report) {
		title, _ := s["title"].(string)

		files := jArray(s, "files")
		if !assert.NotEmpty(t, files,
			"%s: files[] must be non-empty with --per-file", title) {
			continue
		}

		for i, raw := range files {
			entry, ok := raw.(jsonObj)
			if !assert.True(t, ok, "%s: files[%d] must be object", title, i) {
				continue
			}
			for _, key := range required {
				assert.Contains(t, entry, key, "%s: files[%d] must have %q", title, i, key)
			}
		}
	}
}

func TestPerFile_FilePathsRelative(t *testing.T) {
	t.Parallel()

	dir := fixtureDir(t, 3)
	report := runStaticJSON(t, newPerFileStaticService(), dir)

	for _, s := range jSections(t, report) {
		title, _ := s["title"].(string)

		files := jArray(s, "files")
		if !assert.NotEmpty(t, files,
			"%s: files[] must be non-empty with --per-file", title) {
			continue
		}

		for _, raw := range files {
			entry, _ := raw.(jsonObj)
			fp, _ := entry["file_path"].(string)
			assert.False(t, filepath.IsAbs(fp),
				"%s: file_path must be relative, got %q", title, fp)
		}
	}
}

// ---------------------------------------------------------------------------
// Per-file output: IMPORTS (info-only, score -1)
// ---------------------------------------------------------------------------

func TestPerFile_ImportsInfoOnly(t *testing.T) {
	t.Parallel()

	dir := fixtureDir(t, 3)
	report := runStaticJSON(t, newPerFileStaticService(), dir)
	imp := jSectionByTitle(t, jSections(t, report), "IMPORTS")

	score, _ := jFloat(imp["score"])
	assert.InDelta(t, -1.0, score, 0.001, "IMPORTS score must be -1")

	files := jArray(imp, "files")
	if !assert.NotNil(t, files, "IMPORTS must have files[]") {
		return
	}

	for i, fRaw := range files {
		fm, _ := fRaw.(jsonObj)
		fp, _ := fm["file_path"].(string)
		assert.NotEmpty(t, fp, "IMPORTS files[%d] must have file_path", i)

		for j, iRaw := range jArray(fm, "issues") {
			issue, _ := iRaw.(jsonObj)
			loc, _ := issue["location"].(string)
			assert.NotEmpty(t, loc, "IMPORTS files[%d].issues[%d].location must be set", i, j)
		}
	}
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

func TestPerFile_EmptyDir(t *testing.T) {
	t.Parallel()

	dir := fixtureDir(t, 0)
	report := runStaticJSON(t, newPerFileStaticService(), dir)

	for _, s := range jSections(t, report) {
		title, _ := s["title"].(string)
		files := jArray(s, "files")
		assert.NotNil(t, files, "%s: files key must exist even for empty dir", title)
		assert.Empty(t, files, "%s: files[] must be empty for empty dir", title)
	}
}

func TestPerFile_BinaryOnlyDir(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	require.NoError(t, os.WriteFile(
		filepath.Join(dir, "data.bin"), []byte{0x00, 0xFF, 0xFE}, 0o600))

	svc := newStaticService()
	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err, "must not crash on binary-only dir")
	_ = results
}

// ---------------------------------------------------------------------------
// Performance
// ---------------------------------------------------------------------------

func TestPerFile_Performance_Within2xBaseline(t *testing.T) {
	t.Parallel()

	dir := fixtureDir(t, 50)

	measure := func() time.Duration {
		svc := newPerFileStaticService()
		start := time.Now()
		_, err := svc.AnalyzeFolder(context.Background(), dir, nil)
		require.NoError(t, err)
		return time.Since(start)
	}

	baseline := measure()
	perFile := measure()

	t.Logf("baseline=%v per-file=%v", baseline, perFile)
	assert.LessOrEqual(t, perFile, 2*baseline,
		"per-file (%v) must be ≤ 2x baseline (%v)", perFile, baseline)
}

// ---------------------------------------------------------------------------
// Format composability (FR-1.7)
// ---------------------------------------------------------------------------

func TestPerFile_ComposableWithTextAndCompact(t *testing.T) {
	t.Parallel()

	dir := fixtureDir(t, 3)
	svc := newPerFileStaticService()
	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err)

	// Must not crash in any format.
	require.NoError(t, svc.FormatText(results, false, true, nopWriter{}))
	require.NoError(t, svc.FormatCompact(results, true, nopWriter{}))
}

type nopWriter struct{}

func (nopWriter) Write(p []byte) (int, error) { return len(p), nil }

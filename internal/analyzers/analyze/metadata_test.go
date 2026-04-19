package analyze_test

import (
	"encoding/json"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

const testRepoPath = "/home/user/sources/kubernetes"

func TestNewAnalysisMetadata_RepoName(t *testing.T) {
	t.Parallel()

	meta := analyze.NewAnalysisMetadata(testRepoPath)

	assert.Equal(t, "kubernetes", meta.RepoName)
}

func TestNewAnalysisMetadata_RepoPath(t *testing.T) {
	t.Parallel()

	meta := analyze.NewAnalysisMetadata(testRepoPath)

	assert.Equal(t, testRepoPath, meta.RepoPath)
}

func TestNewAnalysisMetadata_AnalyzedAt(t *testing.T) {
	t.Parallel()

	before := time.Now()
	meta := analyze.NewAnalysisMetadata(testRepoPath)
	after := time.Now()

	parsed, err := time.Parse(time.RFC3339, meta.AnalyzedAt)
	require.NoError(t, err)
	assert.False(t, parsed.Before(before.Truncate(time.Second)))
	assert.False(t, parsed.After(after.Add(time.Second)))
}

func TestNewAnalysisMetadata_Version(t *testing.T) {
	t.Parallel()

	meta := analyze.NewAnalysisMetadata(testRepoPath)

	assert.NotEmpty(t, meta.CodefangVersion)
}

func TestUnifiedModel_MetadataInJSON(t *testing.T) {
	t.Parallel()

	model := analyze.UnifiedModel{
		Version:  analyze.UnifiedModelVersion,
		Metadata: analyze.NewAnalysisMetadata(testRepoPath),
		Analyzers: []analyze.AnalyzerResult{
			{ID: "static/test", Mode: analyze.ModeStatic, Report: analyze.Report{}},
		},
	}

	data, err := json.Marshal(model)
	require.NoError(t, err)

	var parsed map[string]any
	require.NoError(t, json.Unmarshal(data, &parsed))

	meta, ok := parsed["metadata"].(map[string]any)
	require.True(t, ok, "metadata section must exist in JSON")
	assert.Equal(t, "kubernetes", meta["repo_name"])
	assert.NotEmpty(t, meta["analyzed_at"])
	assert.NotEmpty(t, meta["codefang_version"])
}

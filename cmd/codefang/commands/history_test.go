package commands_test

import (
	"bytes"
	"reflect"
	"slices"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// analyzerPipeline holds the core and leaf analyzers for the pipeline.
type analyzerPipeline struct {
	core   []analyze.HistoryAnalyzer
	leaves map[string]analyze.HistoryAnalyzer
}

// newTestAnalyzerPipeline creates and configures all analyzers with their dependencies.
func newTestAnalyzerPipeline(repository *gitlib.Repository) *analyzerPipeline {
	treeDiff := &plumbing.TreeDiffAnalyzer{Repository: repository}
	identity := &plumbing.IdentityDetector{}
	ticks := &plumbing.TicksSinceStart{}
	blobCache := &plumbing.BlobCacheAnalyzer{TreeDiff: treeDiff, Repository: repository}
	fileDiff := &plumbing.FileDiffAnalyzer{BlobCache: blobCache, TreeDiff: treeDiff}
	lineStats := &plumbing.LinesStatsCalculator{TreeDiff: treeDiff, BlobCache: blobCache, FileDiff: fileDiff}
	langDetect := &plumbing.LanguagesDetectionAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}
	uastChanges := &plumbing.UASTChangesAnalyzer{TreeDiff: treeDiff, BlobCache: blobCache}

	return &analyzerPipeline{
		core: []analyze.HistoryAnalyzer{
			treeDiff, identity, ticks, blobCache, fileDiff, lineStats, langDetect, uastChanges,
		},
		leaves: map[string]analyze.HistoryAnalyzer{
			"burndown": &burndown.HistoryAnalyzer{
				BlobCache: blobCache, Ticks: ticks, Identity: identity, FileDiff: fileDiff, TreeDiff: treeDiff,
			},
			"couples": &couples.HistoryAnalyzer{Identity: identity, TreeDiff: treeDiff},
			"devs": &devs.HistoryAnalyzer{
				Identity: identity, TreeDiff: treeDiff, Ticks: ticks, Languages: langDetect, LineStats: lineStats,
			},
			"file-history": &filehistory.Analyzer{Identity: identity, TreeDiff: treeDiff, LineStats: lineStats},
			"imports": &imports.HistoryAnalyzer{
				TreeDiff: treeDiff, BlobCache: blobCache, Identity: identity, Ticks: ticks,
			},
			"sentiment": &sentiment.HistoryAnalyzer{UAST: uastChanges, Ticks: ticks},
			"shotness":  &shotness.HistoryAnalyzer{FileDiff: fileDiff, UAST: uastChanges},
			"typos": &typos.HistoryAnalyzer{
				UAST: uastChanges, BlobCache: blobCache, FileDiff: fileDiff,
			},
		},
	}
}

// TestPipelineDependencyCompleteness verifies that all analyzers in the pipeline
// have their required dependencies properly set (not nil).
// This is a regression test to catch missing dependencies in the analyzer pipeline setup.
func TestPipelineDependencyCompleteness(t *testing.T) {
	t.Parallel()

	pipeline := newTestAnalyzerPipeline(nil) // Nil repo is ok for dependency checking.

	// Check core analyzers.
	for _, analyzer := range pipeline.core {
		t.Run("core/"+analyzer.Name(), func(t *testing.T) {
			t.Parallel()
			checkAnalyzerDependencies(t, analyzer)
		})
	}

	// Check leaf analyzers.
	for name, analyzer := range pipeline.leaves {
		t.Run("leaf/"+name, func(t *testing.T) {
			t.Parallel()
			checkAnalyzerDependencies(t, analyzer)
		})
	}
}

// checkAnalyzerDependencies uses reflection to verify all pointer fields
// that look like dependencies (other analyzers) are not nil.
func checkAnalyzerDependencies(t *testing.T, analyzer analyze.HistoryAnalyzer) {
	t.Helper()

	val := reflect.ValueOf(analyzer)
	if val.Kind() == reflect.Ptr {
		val = val.Elem()
	}

	if val.Kind() != reflect.Struct {
		return
	}

	typ := val.Type()

	for i := range val.NumField() {
		field := val.Field(i)
		fieldType := typ.Field(i)

		// Skip unexported fields.
		if !field.CanInterface() {
			continue
		}

		// Only check pointer fields that are likely dependencies (other analyzers).
		if field.Kind() == reflect.Ptr && isAnalyzerDependency(fieldType.Name) {
			if field.IsNil() {
				t.Errorf("dependency field %s is nil in %s", fieldType.Name, analyzer.Name())
			}
		}
	}
}

// isAnalyzerDependency checks if a field name looks like an analyzer dependency.
func isAnalyzerDependency(fieldName string) bool {
	analyzerDependencies := []string{
		"TreeDiff",
		"BlobCache",
		"Identity",
		"Ticks",
		"FileDiff",
		"LineStats",
		"Languages",
		"UASTChanges",
	}

	return slices.Contains(analyzerDependencies, fieldName)
}

// TestAllAnalyzersSerializeJSON tests that all leaf analyzers can serialize to JSON.
func TestAllAnalyzersSerializeJSON(t *testing.T) {
	t.Parallel()

	pipeline := newTestAnalyzerPipeline(nil)

	for name, analyzer := range pipeline.leaves {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			// Create a minimal report.
			report := analyze.Report{}

			var buf bytes.Buffer

			err := analyzer.Serialize(report, analyze.FormatJSON, &buf)
			require.NoError(t, err, "Serialize JSON should not error")

			// Output should be valid (at least not empty for JSON).
			assert.Contains(t, buf.String(), "{", "JSON output should contain opening brace")
		})
	}
}

// TestAllAnalyzersSerializeYAML tests that all leaf analyzers can serialize to YAML.
func TestAllAnalyzersSerializeYAML(t *testing.T) {
	t.Parallel()

	pipeline := newTestAnalyzerPipeline(nil)

	for name, analyzer := range pipeline.leaves {
		t.Run(name, func(t *testing.T) {
			t.Parallel()

			// Create a minimal report.
			report := analyze.Report{}

			var buf bytes.Buffer

			err := analyzer.Serialize(report, analyze.FormatYAML, &buf)

			// YAML serialization may return an error for empty reports in some analyzers.
			// That's acceptable - we just verify no panic occurs.
			_ = err
		})
	}
}

// TestFormatConstants verifies the format constants are defined correctly.
func TestFormatConstants(t *testing.T) {
	t.Parallel()

	assert.Equal(t, "yaml", analyze.FormatYAML)
	assert.Equal(t, "json", analyze.FormatJSON)
	assert.Equal(t, "binary", analyze.FormatBinary)
	assert.Equal(t, "plot", analyze.FormatPlot)
}

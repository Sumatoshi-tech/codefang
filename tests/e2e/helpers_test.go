//go:build e2e

package e2e_test

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/imports"
)

// ---------------------------------------------------------------------------
// Service factory
// ---------------------------------------------------------------------------

// allStaticAnalyzers returns the full set of static analyzers.
func allStaticAnalyzers() []analyze.StaticAnalyzer {
	return []analyze.StaticAnalyzer{
		complexity.NewAnalyzer(),
		comments.NewAnalyzer(),
		halstead.NewAnalyzer(),
		cohesion.NewAnalyzer(),
		imports.NewAnalyzer(),
	}
}

// newStaticService creates a StaticService wired for e2e testing:
// all analyzers, real renderer, no native memory ops.
func newStaticService() *analyze.StaticService {
	svc := analyze.NewStaticService(allStaticAnalyzers(), nil)
	svc.Renderer = &renderer.DefaultStaticRenderer{}
	svc.NativeMemoryReleaseFn = func() {}

	return svc
}

// newPerFileStaticService creates a StaticService with per-file mode enabled.
func newPerFileStaticService() *analyze.StaticService {
	svc := newStaticService()
	svc.PerFile = true

	return svc
}

// ---------------------------------------------------------------------------
// Fixture builder
// ---------------------------------------------------------------------------

// fixtureDir creates a temp directory with n Go source files.
// Each file has 4 functions whose cyclomatic complexity scales with the
// file index, producing non-uniform metric distributions across files.
// All files import "fmt" so the imports analyzer has data.
func fixtureDir(t *testing.T, n int) string {
	t.Helper()

	dir := t.TempDir()

	for i := range n {
		var b strings.Builder
		fmt.Fprintf(&b, "package fixture\n\nimport \"fmt\"\n\n")

		for j := range 4 {
			fmt.Fprintf(&b, "func F%d_%d(a, b int) int {\n\tx := a + b\n", i, j)
			for k := range i + 1 {
				fmt.Fprintf(&b, "\tif x > %d {\n\t\tx += %d\n\t}\n", k, k)
			}
			fmt.Fprintf(&b, "\tfmt.Println(x)\n\treturn x\n}\n\n")
		}

		path := filepath.Join(dir, fmt.Sprintf("file%04d.go", i))
		require.NoError(t, os.WriteFile(path, []byte(b.String()), 0o600))
	}

	return dir
}

// ---------------------------------------------------------------------------
// JSON helpers
// ---------------------------------------------------------------------------

// jsonObj is a convenience alias for navigating parsed JSON.
type jsonObj = map[string]any

// runStaticJSON runs all static analyzers on dir and returns parsed JSON.
func runStaticJSON(t *testing.T, svc *analyze.StaticService, dir string) jsonObj {
	t.Helper()

	results, err := svc.AnalyzeFolder(context.Background(), dir, nil)
	require.NoError(t, err, "AnalyzeFolder")

	var buf bytes.Buffer
	require.NoError(t, svc.FormatJSON(results, &buf), "FormatJSON")

	var out jsonObj
	require.NoError(t, json.Unmarshal(buf.Bytes(), &out), "JSON parse")

	return out
}

// jSections extracts the "sections" array from a top-level report.
func jSections(t *testing.T, report jsonObj) []jsonObj {
	t.Helper()

	raw, ok := report["sections"]
	require.True(t, ok, `top-level "sections" key must exist`)

	arr, ok := raw.([]any)
	require.True(t, ok, `"sections" must be an array`)

	out := make([]jsonObj, 0, len(arr))
	for _, v := range arr {
		m, mOK := v.(jsonObj)
		require.True(t, mOK, "each section must be an object")
		out = append(out, m)
	}

	return out
}

// jSectionByTitle finds a section by its "title" field.
func jSectionByTitle(t *testing.T, secs []jsonObj, title string) jsonObj {
	t.Helper()

	for _, s := range secs {
		if s["title"] == title {
			return s
		}
	}

	t.Fatalf("section %q not found", title)

	return nil
}

// jArray extracts a JSON array by key, returning nil (not fatal) if absent.
func jArray(obj jsonObj, key string) []any {
	raw, ok := obj[key]
	if !ok {
		return nil
	}

	arr, ok := raw.([]any)
	if !ok {
		return nil
	}

	return arr
}

// jMetricLabels returns sorted metric labels from a section's "metrics" array.
func jMetricLabels(section jsonObj) []string {
	arr := jArray(section, "metrics")
	labels := make([]string, 0, len(arr))

	for _, v := range arr {
		m, _ := v.(jsonObj)
		if l, ok := m["label"].(string); ok {
			labels = append(labels, l)
		}
	}

	return labels
}

// jFloat extracts a float64 from a JSON value.
func jFloat(v any) (float64, bool) {
	switch n := v.(type) {
	case float64:
		return n, true
	case json.Number:
		f, err := n.Float64()
		return f, err == nil
	}

	return 0, false
}

// parseMetricValue parses a metric "value" string (e.g. "1,234") as float64.
func parseMetricValue(v any) (float64, bool) {
	s, ok := v.(string)
	if !ok {
		return jFloat(v)
	}

	cleaned := strings.NewReplacer(",", "", "%", "", " ", "").Replace(s)

	var f float64
	if _, err := fmt.Sscanf(cleaned, "%f", &f); err != nil {
		return math.NaN(), false
	}

	return f, true
}

// avg computes the arithmetic mean of a float slice.
func avg(vals []float64) float64 {
	if len(vals) == 0 {
		return 0
	}

	sum := 0.0
	for _, v := range vals {
		sum += v
	}

	return sum / float64(len(vals))
}

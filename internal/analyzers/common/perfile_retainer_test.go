package common

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

func TestPerFileRetainer_Disabled_ReturnsNil(t *testing.T) {
	t.Parallel()

	var retainer PerFileRetainer

	retainer.Retain(analyze.Report{"total_functions": 5})

	assert.Nil(t, retainer.PerFileResults())
}

func TestPerFileRetainer_Enabled_RetainsThreeFiles(t *testing.T) {
	t.Parallel()

	var retainer PerFileRetainer
	retainer.SetPerFileMode(true)

	retainer.Retain(analyze.Report{
		"total_functions": 3,
		"functions":       analyze.TypedCollection{SourceFile: "/repo/a.go"},
	})
	retainer.Retain(analyze.Report{
		"total_functions": 5,
		"functions":       analyze.TypedCollection{SourceFile: "/repo/b.go"},
	})
	retainer.Retain(analyze.Report{
		"total_functions": 2,
		"functions":       analyze.TypedCollection{SourceFile: "/repo/c.go"},
	})

	results := retainer.PerFileResults()
	require.Len(t, results, 3)
	assert.Contains(t, results, "/repo/a.go")
	assert.Contains(t, results, "/repo/b.go")
	assert.Contains(t, results, "/repo/c.go")
	assert.Equal(t, 3, results["/repo/a.go"]["total_functions"])
}

func TestPerFileRetainer_LegacyMapSlice(t *testing.T) {
	t.Parallel()

	var retainer PerFileRetainer
	retainer.SetPerFileMode(true)

	retainer.Retain(analyze.Report{
		"functions": []map[string]any{
			{"name": "Foo", analyze.SourceFileKey: "/repo/legacy.go"},
		},
	})

	results := retainer.PerFileResults()
	require.Len(t, results, 1)
	assert.Contains(t, results, "/repo/legacy.go")
}

func TestPerFileRetainer_NilReport(t *testing.T) {
	t.Parallel()

	var retainer PerFileRetainer
	retainer.SetPerFileMode(true)

	retainer.Retain(nil)

	assert.Nil(t, retainer.PerFileResults())
}

func TestPerFileRetainer_NoSourceFile(t *testing.T) {
	t.Parallel()

	var retainer PerFileRetainer
	retainer.SetPerFileMode(true)

	retainer.Retain(analyze.Report{"total_functions": 5})

	assert.Nil(t, retainer.PerFileResults())
}

func TestPerFileRetainer_CloneIsolation(t *testing.T) {
	t.Parallel()

	var retainer PerFileRetainer
	retainer.SetPerFileMode(true)

	report := analyze.Report{
		"count":     10,
		"functions": analyze.TypedCollection{SourceFile: "/repo/x.go"},
	}

	retainer.Retain(report)

	// Mutate original — retained copy must not change.
	report["count"] = 999

	results := retainer.PerFileResults()
	assert.Equal(t, 10, results["/repo/x.go"]["count"])
}

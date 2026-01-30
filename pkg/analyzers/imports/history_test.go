package imports //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	if h.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{
		TreeDiff:  &plumbing.TreeDiffAnalyzer{},
		BlobCache: &plumbing.BlobCacheAnalyzer{},
	}
	err := h.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}

	opts := h.ListConfigurationOptions()
	if len(opts) == 0 {
		t.Error("expected options")
	}
}

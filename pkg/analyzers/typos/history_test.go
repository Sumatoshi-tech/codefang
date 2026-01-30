package typos //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"
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

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	opts := h.ListConfigurationOptions()
	_ = opts
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	err := h.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	h := &HistoryAnalyzer{}
	require.NoError(t, h.Initialize(nil))

	clones := h.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

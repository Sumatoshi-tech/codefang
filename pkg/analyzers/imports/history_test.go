package imports

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()
	if h.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()
	if h.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()
	err := h.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	h := NewHistoryAnalyzer()

	opts := h.ListConfigurationOptions()
	if len(opts) == 0 {
		t.Error("expected options")
	}
}

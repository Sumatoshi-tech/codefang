package burndown //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	if b.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}

	opts := b.ListConfigurationOptions()
	if len(opts) == 0 {
		t.Error("expected options")
	}
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{}
	err := b.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	b := &HistoryAnalyzer{
		Granularity: 30,
		Sampling:    30,
		Goroutines:  4,
	}
	err := b.Initialize(nil)
	require.NoError(t, err)
}

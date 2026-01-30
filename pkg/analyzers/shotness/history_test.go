package shotness //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestHistoryAnalyzer_Name(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Name() == "" {
		t.Error("Name empty")
	}
}

func TestHistoryAnalyzer_Flag(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Flag() == "" {
		t.Error("Flag empty")
	}
}

func TestHistoryAnalyzer_Description(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	if s.Description() == "" {
		t.Error("Description empty")
	}
}

func TestHistoryAnalyzer_ListConfigurationOptions(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	opts := s.ListConfigurationOptions()
	// May or may not have options.
	_ = opts
}

func TestHistoryAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	err := s.Configure(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	err := s.Initialize(nil)
	require.NoError(t, err)
}

func TestHistoryAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	s := &HistoryAnalyzer{}
	require.NoError(t, s.Initialize(nil))

	clones := s.Fork(2)
	if len(clones) != 2 {
		t.Error("expected 2 clones")
	}
}

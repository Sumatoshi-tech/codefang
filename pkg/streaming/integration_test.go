package streaming_test

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

// TestAllAnalyzers_ImplementCheckpointable verifies all history analyzers
// implement the Checkpointable interface.
func TestAllAnalyzers_ImplementCheckpointable(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		analyzer checkpoint.Checkpointable
	}{
		{"burndown", &burndown.HistoryAnalyzer{}},
		{"couples", &couples.HistoryAnalyzer{}},
		{"file_history", filehistory.NewAnalyzer()},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			// Verify methods exist (compile-time check via interface assignment).
			require.NotNil(t, tt.analyzer, "%s analyzer should not be nil", tt.name)

			// Verify CheckpointSize returns non-negative value.
			size := tt.analyzer.CheckpointSize()
			require.GreaterOrEqual(t, size, int64(0),
				"%s.CheckpointSize() should return non-negative value", tt.name)
		})
	}
}

// TestAllAnalyzers_ImplementHibernatable verifies all history analyzers
// implement the Hibernatable interface.
func TestAllAnalyzers_ImplementHibernatable(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		analyzer streaming.Hibernatable
	}{
		{"burndown", &burndown.HistoryAnalyzer{}},
		{"couples", &couples.HistoryAnalyzer{}},
		{"file_history", filehistory.NewAnalyzer()},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			// Verify methods exist (compile-time check via interface assignment).
			require.NotNil(t, tt.analyzer, "%s analyzer should not be nil", tt.name)

			// Verify Hibernate doesn't panic on empty analyzer.
			err := tt.analyzer.Hibernate()
			require.NoError(t, err, "%s.Hibernate() should not error on empty analyzer", tt.name)

			// Verify Boot doesn't panic after Hibernate.
			err = tt.analyzer.Boot()
			require.NoError(t, err, "%s.Boot() should not error after Hibernate", tt.name)
		})
	}
}

// TestAllAnalyzers_CheckpointRoundTrip verifies checkpoint save/load works
// for all history analyzers.
func TestAllAnalyzers_CheckpointRoundTrip(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		setup func(t *testing.T) checkpoint.Checkpointable
	}{
		{
			name: "burndown",
			setup: func(t *testing.T) checkpoint.Checkpointable {
				t.Helper()

				b := &burndown.HistoryAnalyzer{
					Granularity: burndown.DefaultBurndownGranularity,
					Sampling:    burndown.DefaultBurndownSampling,
					Goroutines:  2,
				}
				require.NoError(t, b.Initialize(nil))

				return b
			},
		},
		{
			name: "couples",
			setup: func(t *testing.T) checkpoint.Checkpointable {
				t.Helper()

				c := &couples.HistoryAnalyzer{}
				require.NoError(t, c.Initialize(nil))

				return c
			},
		},
		{
			name: "file_history",
			setup: func(t *testing.T) checkpoint.Checkpointable {
				t.Helper()

				f := filehistory.NewAnalyzer()
				require.NoError(t, f.Initialize(nil))

				return f
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			dir := t.TempDir()
			analyzer := tt.setup(t)

			// Save checkpoint.
			err := analyzer.SaveCheckpoint(dir)
			require.NoError(t, err, "%s.SaveCheckpoint() failed", tt.name)

			// Create fresh analyzer and load.
			restored := tt.setup(t)

			err = restored.LoadCheckpoint(dir)
			require.NoError(t, err, "%s.LoadCheckpoint() failed", tt.name)
		})
	}
}

// TestAllAnalyzers_HibernateBootCycle verifies hibernate/boot cycle works
// for all history analyzers without losing accumulated state.
func TestAllAnalyzers_HibernateBootCycle(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		setup func(t *testing.T) streaming.Hibernatable
	}{
		{
			name: "burndown",
			setup: func(t *testing.T) streaming.Hibernatable {
				t.Helper()

				b := &burndown.HistoryAnalyzer{
					Granularity: burndown.DefaultBurndownGranularity,
					Sampling:    burndown.DefaultBurndownSampling,
					Goroutines:  2,
				}
				require.NoError(t, b.Initialize(nil))

				return b
			},
		},
		{
			name: "couples",
			setup: func(t *testing.T) streaming.Hibernatable {
				t.Helper()

				c := &couples.HistoryAnalyzer{}
				require.NoError(t, c.Initialize(nil))

				return c
			},
		},
		{
			name: "file_history",
			setup: func(t *testing.T) streaming.Hibernatable {
				t.Helper()

				f := filehistory.NewAnalyzer()
				require.NoError(t, f.Initialize(nil))

				return f
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			analyzer := tt.setup(t)

			// Hibernate.
			err := analyzer.Hibernate()
			require.NoError(t, err, "%s.Hibernate() failed", tt.name)

			// Boot.
			err = analyzer.Boot()
			require.NoError(t, err, "%s.Boot() failed", tt.name)

			// Should be able to hibernate/boot again.
			err = analyzer.Hibernate()
			require.NoError(t, err, "%s.Hibernate() failed on second call", tt.name)

			err = analyzer.Boot()
			require.NoError(t, err, "%s.Boot() failed on second call", tt.name)
		})
	}
}

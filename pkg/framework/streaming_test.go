package framework

import "testing"

func TestCanResumeWithCheckpoint(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name              string
		totalAnalyzers    int
		checkpointable    int
		wantResumeEnabled bool
	}{
		{
			name:              "no analyzers",
			totalAnalyzers:    0,
			checkpointable:    0,
			wantResumeEnabled: false,
		},
		{
			name:              "none checkpointable",
			totalAnalyzers:    8,
			checkpointable:    0,
			wantResumeEnabled: false,
		},
		{
			name:              "partial checkpoint support",
			totalAnalyzers:    8,
			checkpointable:    3,
			wantResumeEnabled: false,
		},
		{
			name:              "all analyzers checkpointable",
			totalAnalyzers:    8,
			checkpointable:    8,
			wantResumeEnabled: true,
		},
		{
			name:              "checkpoint count exceeds analyzers",
			totalAnalyzers:    8,
			checkpointable:    9,
			wantResumeEnabled: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := CanResumeWithCheckpoint(tt.totalAnalyzers, tt.checkpointable)
			if got != tt.wantResumeEnabled {
				t.Fatalf(
					"CanResumeWithCheckpoint(%d, %d) = %t, want %t",
					tt.totalAnalyzers,
					tt.checkpointable,
					got,
					tt.wantResumeEnabled,
				)
			}
		})
	}
}

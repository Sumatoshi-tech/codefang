// FRD: specs/frds/FRD-20260302-typed-fact-accessors.md.
package plumbing_test

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/identity"
	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestGetTickSize(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		facts map[string]any
		want  time.Duration
		ok    bool
	}{
		{
			name:  "present_with_correct_type",
			facts: map[string]any{plumbing.FactTickSize: 24 * time.Hour},
			want:  24 * time.Hour,
			ok:    true,
		},
		{
			name:  "absent",
			facts: map[string]any{},
			want:  0,
			ok:    false,
		},
		{
			name:  "wrong_type",
			facts: map[string]any{plumbing.FactTickSize: "not a duration"},
			want:  0,
			ok:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, ok := plumbing.GetTickSize(tt.facts)
			assert.Equal(t, tt.ok, ok)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestGetCommitsByTick(t *testing.T) {
	t.Parallel()

	sampleHash := gitlib.Hash{0x01}
	sampleMap := map[int][]gitlib.Hash{0: {sampleHash}}

	tests := []struct {
		name  string
		facts map[string]any
		want  map[int][]gitlib.Hash
		ok    bool
	}{
		{
			name:  "present_with_correct_type",
			facts: map[string]any{plumbing.FactCommitsByTick: sampleMap},
			want:  sampleMap,
			ok:    true,
		},
		{
			name:  "absent",
			facts: map[string]any{},
			want:  nil,
			ok:    false,
		},
		{
			name:  "wrong_type",
			facts: map[string]any{plumbing.FactCommitsByTick: 42},
			want:  nil,
			ok:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, ok := plumbing.GetCommitsByTick(tt.facts)
			assert.Equal(t, tt.ok, ok)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestGetReversedPeopleDict(t *testing.T) {
	t.Parallel()

	sampleDict := []string{"alice", "bob"}

	tests := []struct {
		name  string
		facts map[string]any
		want  []string
		ok    bool
	}{
		{
			name:  "present_with_correct_type",
			facts: map[string]any{identity.FactIdentityDetectorReversedPeopleDict: sampleDict},
			want:  sampleDict,
			ok:    true,
		},
		{
			name:  "absent",
			facts: map[string]any{},
			want:  nil,
			ok:    false,
		},
		{
			name:  "wrong_type",
			facts: map[string]any{identity.FactIdentityDetectorReversedPeopleDict: 42},
			want:  nil,
			ok:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, ok := plumbing.GetReversedPeopleDict(tt.facts)
			assert.Equal(t, tt.ok, ok)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestGetPeopleCount(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		facts map[string]any
		want  int
		ok    bool
	}{
		{
			name:  "present_with_correct_type",
			facts: map[string]any{identity.FactIdentityDetectorPeopleCount: 5},
			want:  5,
			ok:    true,
		},
		{
			name:  "absent",
			facts: map[string]any{},
			want:  0,
			ok:    false,
		},
		{
			name:  "wrong_type",
			facts: map[string]any{identity.FactIdentityDetectorPeopleCount: "five"},
			want:  0,
			ok:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got, ok := plumbing.GetPeopleCount(tt.facts)
			assert.Equal(t, tt.ok, ok)
			assert.Equal(t, tt.want, got)
		})
	}
}

// FRD: specs/frds/FRD-20260302-identity-mixin.md.
package common_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"
)

func TestIdentityMixin_GetReversedPeopleDict(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		mixin    common.IdentityMixin
		expected []string
	}{
		{
			name: "identity_available_with_non_empty_dict",
			mixin: common.IdentityMixin{
				Identity: &plumbing.IdentityDetector{
					ReversedPeopleDict: []string{"Alice", "Bob"},
				},
				ReversedPeopleDict: []string{"fallback"},
			},
			expected: []string{"Alice", "Bob"},
		},
		{
			name: "identity_available_with_empty_dict",
			mixin: common.IdentityMixin{
				Identity: &plumbing.IdentityDetector{
					ReversedPeopleDict: []string{},
				},
				ReversedPeopleDict: []string{"fallback"},
			},
			expected: []string{"fallback"},
		},
		{
			name: "identity_nil",
			mixin: common.IdentityMixin{
				ReversedPeopleDict: []string{"fallback"},
			},
			expected: []string{"fallback"},
		},
		{
			name:     "both_nil",
			mixin:    common.IdentityMixin{},
			expected: nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			result := tt.mixin.GetReversedPeopleDict()
			assert.Equal(t, tt.expected, result)
		})
	}
}

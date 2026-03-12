package common

import "github.com/Sumatoshi-tech/codefang/internal/analyzers/plumbing"

// IdentityMixin deduplicates the identity-resolution pattern shared by
// burndown, couples, imports, and devs history analyzers.
//
// Each of those analyzers needs two fields — an IdentityDetector reference
// (set by the pipeline) and a fallback reversed-people-dict (set from
// Configure facts). The GetReversedPeopleDict method encapsulates the
// two-tier resolution: prefer IdentityDetector's dict when available,
// fall back to the manually-set ReversedPeopleDict.
type IdentityMixin struct {
	Identity           *plumbing.IdentityDetector
	ReversedPeopleDict []string
}

// GetReversedPeopleDict returns the identity-resolved people dictionary.
// It prefers IdentityDetector's dict when available and non-empty,
// falling back to the manually-set ReversedPeopleDict.
func (m *IdentityMixin) GetReversedPeopleDict() []string {
	if m.Identity != nil && len(m.Identity.ReversedPeopleDict) > 0 {
		return m.Identity.ReversedPeopleDict
	}

	return m.ReversedPeopleDict
}

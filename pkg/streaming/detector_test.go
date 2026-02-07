package streaming

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestDetector_SmallRepo_NoStream(t *testing.T) {
	d := Detector{
		CommitCount:  1000,
		MemoryBudget: 0, // No budget constraint
	}
	assert.False(t, d.ShouldStream())
}

func TestDetector_LargeRepo_ShouldStream(t *testing.T) {
	d := Detector{
		CommitCount:  DefaultCommitThreshold,
		MemoryBudget: 0,
	}
	assert.True(t, d.ShouldStream())
}

func TestDetector_JustBelowThreshold_NoStream(t *testing.T) {
	d := Detector{
		CommitCount:  DefaultCommitThreshold - 1,
		MemoryBudget: 0,
	}
	assert.False(t, d.ShouldStream())
}

func TestDetector_BudgetExceeded_ShouldStream(t *testing.T) {
	// Small repo but tight budget: estimated memory exceeds budget
	// With 10000 commits at ~2KiB per commit state growth = 20MiB
	// Plus base overhead ~50MiB = ~70MiB estimated
	// Budget of 60MiB should trigger streaming
	d := Detector{
		CommitCount:  10000,
		MemoryBudget: 60 * 1024 * 1024, // 60 MiB - tight budget
	}
	assert.True(t, d.ShouldStream())
}

func TestDetector_BudgetSufficient_NoStream(t *testing.T) {
	// Small repo with ample budget should not stream
	d := Detector{
		CommitCount:  10000,
		MemoryBudget: 512 * 1024 * 1024, // 512 MiB - plenty of room
	}
	assert.False(t, d.ShouldStream())
}

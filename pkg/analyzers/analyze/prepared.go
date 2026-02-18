package analyze

import (
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// PreparedCommit holds all pre-computed data for a single commit.
// This is used by the pipelined runner to pass pre-fetched data to analyzers.
type PreparedCommit struct {
	Ctx       *Context
	Changes   []*gitlib.Change
	Cache     map[gitlib.Hash]*gitlib.CachedBlob
	FileDiffs map[string]pkgplumbing.FileDiffData
	Index     int
	Err       error
	// AuthorID is the resolved author identifier.
	AuthorID int
	// Tick is the time tick for this commit.
	Tick int
}

// PreparationConfig holds configuration for commit preparation.
type PreparationConfig struct {
	// Tick0 is the time of the first commit (for tick calculation).
	Tick0 time.Time
	// TickSize is the duration of one tick.
	TickSize time.Duration
	// PeopleDict maps author keys to author IDs.
	PeopleDict map[string]int
}

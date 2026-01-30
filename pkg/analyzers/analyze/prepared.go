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

// PreparedConsumer is implemented by analyzers that can consume PreparedCommit.
// This enables pipelined execution where commit preparation runs in parallel.
type PreparedConsumer interface {
	ConsumePrepared(prepared *PreparedCommit) error
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

// ComputeTick calculates the tick value for a given commit time.
func (pc *PreparationConfig) ComputeTick(commitTime time.Time) int {
	if pc.Tick0.IsZero() {
		return 0
	}

	elapsed := commitTime.Sub(pc.Tick0)
	if elapsed < 0 {
		return 0
	}

	return int(elapsed / pc.TickSize)
}

// ComputeAuthorID resolves the author ID from a commit signature.
func (pc *PreparationConfig) ComputeAuthorID(sig gitlib.Signature) int {
	if pc.PeopleDict == nil {
		return 0
	}

	key := sig.Email
	if id, ok := pc.PeopleDict[key]; ok {
		return id
	}

	return 0
}

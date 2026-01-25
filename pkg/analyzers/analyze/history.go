package analyze

import (
	"io"
	"time"

	"github.com/go-git/go-git/v6"
	"github.com/go-git/go-git/v6/plumbing/object"
)

// Context provides information about the current step in the analysis.
type Context struct {
	Commit  *object.Commit
	Index   int
	IsMerge bool
	Time    time.Time
}

// HistoryAnalyzer interface defines the contract for history-based analyzers.
type HistoryAnalyzer interface {
	Analyzer

	// Core analysis methods
	Initialize(repository *git.Repository) error

	// Consumption
	Consume(ctx *Context) error

	// Result handling
	Finalize() (Report, error)

	// Branching support
	Fork(n int) []HistoryAnalyzer
	Merge(branches []HistoryAnalyzer)

	// Formatting/Serialization
	Serialize(result Report, binary bool, writer io.Writer) error
}

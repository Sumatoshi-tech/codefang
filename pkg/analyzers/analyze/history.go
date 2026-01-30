package analyze

import (
	"io"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// Serialization format constants.
const (
	FormatYAML   = "yaml"
	FormatJSON   = "json"
	FormatBinary = "binary"
)

// CommitIdentity provides commit identification methods.
type CommitIdentity interface {
	Hash() gitlib.Hash
	Author() gitlib.Signature
	Committer() gitlib.Signature
	Message() string
}

// CommitParents provides access to parent commits.
type CommitParents interface {
	NumParents() int
	Parent(n int) (*gitlib.Commit, error)
}

// CommitLike is an interface for commit-like objects (real or mock).
// It composes CommitIdentity and CommitParents with tree/file access.
type CommitLike interface {
	CommitIdentity
	CommitParents
	Tree() (*gitlib.Tree, error)
	Files() (*gitlib.FileIter, error)
	File(path string) (*gitlib.File, error)
}

// Context provides information about the current step in the analysis.
type Context struct {
	Time    time.Time
	Commit  CommitLike
	Index   int
	IsMerge bool
}

// HistoryAnalyzer interface defines the contract for history-based analyzers.
type HistoryAnalyzer interface { //nolint:interfacebloat // interface methods are all needed.
	Analyzer

	// Core analysis methods.
	Initialize(repository *gitlib.Repository) error

	// Consumption.
	Consume(ctx *Context) error

	// Result handling.
	Finalize() (Report, error)

	// Branching support.
	Fork(n int) []HistoryAnalyzer
	Merge(branches []HistoryAnalyzer)

	// Formatting/Serialization.
	// Format can be: "yaml", "json", or "binary" (protobuf).
	Serialize(result Report, format string, writer io.Writer) error
}

package filehistory

import (
	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// PathAction represents a file path change in a single commit.
type PathAction struct {
	Path       string
	Action     gitlib.ChangeAction
	CommitHash gitlib.Hash
	FromPath   string // For renames: source path.
	ToPath     string // For renames: destination path.
}

// LineStatUpdate represents line stat delta for one file/author in a commit.
type LineStatUpdate struct {
	Path     string
	AuthorID int
	Stats    plumbing.LineStats
}

// CategoryCounts holds file category counts for a single commit.
type CategoryCounts struct {
	Source        int `json:"source"`
	Vendor        int `json:"vendor"`
	Generated     int `json:"generated"`
	Documentation int `json:"documentation"`
	Configuration int `json:"configuration"`
	Image         int `json:"image"`
	DotFile       int `json:"dotfile"`
	Binary        int `json:"binary"`
}

// Add adds the counts from other into c.
func (c *CategoryCounts) Add(other *CategoryCounts) {
	c.Source += other.Source
	c.Vendor += other.Vendor
	c.Generated += other.Generated
	c.Documentation += other.Documentation
	c.Configuration += other.Configuration
	c.Image += other.Image
	c.DotFile += other.DotFile
	c.Binary += other.Binary
}

// Total returns the sum of all category counts.
func (c *CategoryCounts) Total() int {
	return c.Source + c.Vendor + c.Generated + c.Documentation +
		c.Configuration + c.Image + c.DotFile + c.Binary
}

// Get returns the count for the given category.
func (c *CategoryCounts) Get(cat Category) int {
	switch cat {
	case CategorySource:
		return c.Source
	case CategoryVendor:
		return c.Vendor
	case CategoryGenerated:
		return c.Generated
	case CategoryDocumentation:
		return c.Documentation
	case CategoryConfiguration:
		return c.Configuration
	case CategoryImage:
		return c.Image
	case CategoryDotFile:
		return c.DotFile
	case CategoryBinary:
		return c.Binary
	default:
		return 0
	}
}

// Increment adds one to the count for the given category.
func (c *CategoryCounts) Increment(cat Category) {
	switch cat {
	case CategorySource:
		c.Source++
	case CategoryVendor:
		c.Vendor++
	case CategoryGenerated:
		c.Generated++
	case CategoryDocumentation:
		c.Documentation++
	case CategoryConfiguration:
		c.Configuration++
	case CategoryImage:
		c.Image++
	case CategoryDotFile:
		c.DotFile++
	case CategoryBinary:
		c.Binary++
	}
}

// CommitData is the per-commit TC payload emitted by Consume().
// It captures path actions (insert/modify/delete/rename), line stat deltas,
// and file category composition.
type CommitData struct {
	PathActions     []PathAction
	LineStatUpdates []LineStatUpdate
	Composition     CategoryCounts
}

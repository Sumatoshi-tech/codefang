package filehistory

import (
	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/pkg/pathfilter"
)

// Category represents a file classification category.
type Category string

// File classification categories.
const (
	CategorySource        Category = "source"
	CategoryVendor        Category = "vendor"
	CategoryGenerated     Category = "generated"
	CategoryDocumentation Category = "documentation"
	CategoryConfiguration Category = "configuration"
	CategoryImage         Category = "image"
	CategoryDotFile       Category = "dotfile"
	CategoryBinary        Category = "binary"
)

// AllCategories is the canonical order for display and charting.
var AllCategories = []Category{
	CategorySource, CategoryDocumentation, CategoryConfiguration,
	CategoryVendor, CategoryGenerated, CategoryDotFile, CategoryImage, CategoryBinary,
}

// Classifier categorizes files using enry and pathfilter.
type Classifier struct {
	filter *pathfilter.Filter
}

// NewClassifier creates a Classifier with default rules.
func NewClassifier() *Classifier {
	return &Classifier{filter: pathfilter.New()}
}

// Classify returns the category for a file. Content can be nil for path-only classification.
// Categories are checked in priority order (first match wins):
// Binary > Image > Vendor > Generated > Documentation > Configuration > DotFile > Source.
func (c *Classifier) Classify(filePath string, content []byte) Category {
	if len(content) > 0 && enry.IsBinary(content) {
		return CategoryBinary
	}

	if enry.IsImage(filePath) {
		return CategoryImage
	}

	if enry.IsVendor(filePath) {
		return CategoryVendor
	}

	if c.filter.IsGeneratedPath(filePath) {
		return CategoryGenerated
	}

	if len(content) > 0 && c.filter.IsGeneratedContent(content) {
		return CategoryGenerated
	}

	if enry.IsDocumentation(filePath) {
		return CategoryDocumentation
	}

	if enry.IsConfiguration(filePath) {
		return CategoryConfiguration
	}

	if enry.IsDotFile(filePath) {
		return CategoryDotFile
	}

	return CategorySource
}

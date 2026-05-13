// Package pathpolicy decides whether a file path should be excluded
// from analysis based on user-visible options that mirror the CLI
// flags (--include-vendored, --include-generated,
// --extra-excluded-prefixes). Pure, stateless, cross-phase.
package pathpolicy

import (
	"strings"

	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/pkg/pathfilter"
)

// defaultFilter carries the built-in generated-file heuristics
// (filename suffixes, prefixes, and content markers) as they ship in
// pkg/pathfilter. Reusing one immutable instance keeps allocation
// off the hot path.
var defaultFilter = pathfilter.New()

// Options captures the user-visible configuration.
// The zero value excludes vendor, generated, and nothing else.
type Options struct {
	IncludeVendored       bool
	IncludeGenerated      bool
	ExtraExcludedPrefixes []string
}

// Exclude reports whether the given path should be skipped.
// content may be nil; when provided, content-based heuristics may
// refine the generated-file classification.
func Exclude(path string, content []byte, opts Options) bool {
	switch {
	case matchesAnyPrefix(path, opts.ExtraExcludedPrefixes):
		return true
	case !opts.IncludeVendored && enry.IsVendor(path):
		return true
	case !opts.IncludeGenerated && isGenerated(path, content):
		return true
	}

	return false
}

// matchesAnyPrefix returns true if path begins with any non-empty
// entry of prefixes.
func matchesAnyPrefix(path string, prefixes []string) bool {
	for _, prefix := range prefixes {
		if prefix != "" && strings.HasPrefix(path, prefix) {
			return true
		}
	}

	return false
}

// isGenerated returns true if the path or header content identifies
// the file as machine-generated per the built-in heuristics.
func isGenerated(path string, content []byte) bool {
	if defaultFilter.IsGeneratedPath(path) {
		return true
	}

	return len(content) > 0 && defaultFilter.IsGeneratedContent(content)
}

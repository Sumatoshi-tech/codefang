package analyze

// Static-side --languages filter.

import "path/filepath"

// matchesLanguageGlobs reports whether name's basename matches any of
// the given fnmatch-style globs. An empty or nil globs slice disables
// filtering and returns true.
func matchesLanguageGlobs(name string, globs []string) bool {
	if len(globs) == 0 {
		return true
	}

	base := filepath.Base(name)
	for _, g := range globs {
		ok, err := filepath.Match(g, base)
		if err == nil && ok {
			return true
		}
	}

	return false
}

package analyze

// LanguageGlobMatcher exposes matchesLanguageGlobs for black-box tests
// in the analyze_test package.
func LanguageGlobMatcher(name string, globs []string) bool {
	return matchesLanguageGlobs(name, globs)
}

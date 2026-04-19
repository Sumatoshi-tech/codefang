// Package langpath converts user-supplied language tokens into
// deterministic pathspec globs backed by enry's Linguist data.
//
// See FRD: specs/frds/FRD-20260419-pathspec-builder.md.
package langpath

import (
	"errors"
	"fmt"
	"slices"
	"strings"

	"github.com/src-d/enry/v2"
	"github.com/src-d/enry/v2/data"
)

// ErrUnknownLanguage is returned when a user-supplied token does not
// resolve to any Linguist language (including its aliases).
var ErrUnknownLanguage = errors.New("unknown language")

// filenamesByLanguage inverts enry.data.LanguagesByFilename so we can
// look up "languages → []filename" at Globs time. Built once at
// package load; read-only thereafter.
var filenamesByLanguage = invertLanguagesByFilename()

func invertLanguagesByFilename() map[string][]string {
	out := make(map[string][]string)

	for filename, langs := range data.LanguagesByFilename {
		for _, lang := range langs {
			out[lang] = append(out[lang], filename)
		}
	}

	return out
}

const (
	// allToken is the sentinel meaning "do not restrict by language".
	allToken = "all"
	// extensionGlobPrefix is prepended to every extension-derived glob.
	extensionGlobPrefix = "*"
)

// Globs converts a list of user-supplied language tokens into a
// sorted, deduplicated set of pathspec globs. wantsAll is true when
// the caller did not restrict languages (empty input or the literal
// "all" token). Callers should skip path-spec push-down in that case.
func Globs(langs []string) (globs []string, wantsAll bool, err error) {
	if len(langs) == 0 {
		return nil, true, nil
	}

	set := make(map[string]struct{})

	for _, raw := range langs {
		token := strings.TrimSpace(raw)
		if strings.EqualFold(token, allToken) {
			return nil, true, nil
		}

		canonical, ok := enry.GetLanguageByAlias(token)
		if !ok {
			return nil, false, fmt.Errorf("%w: %q", ErrUnknownLanguage, raw)
		}

		for _, ext := range enry.GetLanguageExtensions(canonical) {
			set[extensionGlobPrefix+ext] = struct{}{}
		}

		for _, name := range filenamesByLanguage[canonical] {
			set[name] = struct{}{}
		}
	}

	out := make([]string, 0, len(set))
	for g := range set {
		out = append(out, g)
	}

	slices.Sort(out)

	return out, false, nil
}

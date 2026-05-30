// Command enrydump emits the three enry Linguist data tables consumed by the
// Rust cf-langpath crate as a tab-separated snapshot. See
// rust/crates/cf-langpath/data/README.md for the format and provenance. This is
// a dev-only regeneration tool; it is not part of any build.
//
// Usage:
//
//	go run ./tools/enrydump rust/crates/cf-langpath/data/enry-v2.1.0.tsv
package main

import (
	"fmt"
	"os"
	"sort"

	"github.com/src-d/enry/v2/data"
)

func main() {
	out := "/tmp/enry_vendor.tsv"
	if len(os.Args) > 1 {
		out = os.Args[1]
	}
	f, err := os.Create(out)
	if err != nil {
		panic(err)
	}
	defer f.Close()

	// Section A: alias key -> canonical language. This is exactly the map enry's
	// GetLanguageByAlias consults (data.LanguageByAliasMap); its keys are already
	// normalized (lowercased, spaces->'_').
	akeys := make([]string, 0, len(data.LanguageByAliasMap))
	for k := range data.LanguageByAliasMap {
		akeys = append(akeys, k)
	}
	sort.Strings(akeys)
	for _, k := range akeys {
		fmt.Fprintf(f, "A\t%s\t%s\n", k, data.LanguageByAliasMap[k])
	}

	// Section E: canonical language -> extensions (each with leading dot). This
	// is data.ExtensionsByLanguage, the exact map enry's GetLanguageExtensions
	// reads.
	ekeys := make([]string, 0, len(data.ExtensionsByLanguage))
	for k := range data.ExtensionsByLanguage {
		ekeys = append(ekeys, k)
	}
	sort.Strings(ekeys)
	for _, k := range ekeys {
		fmt.Fprintf(f, "E\t%s", k)
		for _, e := range data.ExtensionsByLanguage[k] {
			fmt.Fprintf(f, "\t%s", e)
		}
		fmt.Fprintln(f)
	}

	// Section F: literal filename -> canonical language(s) (data.LanguagesByFilename).
	fkeys := make([]string, 0, len(data.LanguagesByFilename))
	for k := range data.LanguagesByFilename {
		fkeys = append(fkeys, k)
	}
	sort.Strings(fkeys)
	for _, k := range fkeys {
		fmt.Fprintf(f, "F\t%s", k)
		for _, l := range data.LanguagesByFilename[k] {
			fmt.Fprintf(f, "\t%s", l)
		}
		fmt.Fprintln(f)
	}

	fmt.Printf("aliases=%d extensions=%d filenames=%d\n",
		len(akeys), len(ekeys), len(fkeys))
}

// Command enrydump writes the vendored enry v2.1.0 tables that cf-langpath
// embeds. It must run inside the enry module so `github.com/src-d/enry/v2/data`
// resolves to the pinned version:
//
//	go run ./tools/enrydump <lookup.tsv> <types.tsv>   (from the enry module root)
//
// lookup.tsv receives the alias (A), extension (E) and filename (F) records;
// types.tsv receives the Linguist language type (T) of every language. Keys
// are sorted, so the output is byte-stable.
package main

import (
	"fmt"
	"os"
	"sort"

	"github.com/src-d/enry/v2/data"
)

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintln(os.Stderr, "usage: enrydump <lookup.tsv> <types.tsv>")
		os.Exit(2)
	}
	if err := writeLookup(os.Args[1]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	if err := writeTypes(os.Args[2]); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func writeLookup(path string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()

	for _, k := range sortedAliasKeys() {
		fmt.Fprintf(f, "A\t%s\t%s\n", k, data.LanguageByAliasMap[k])
	}
	for _, k := range sortedExtensionKeys() {
		fmt.Fprintf(f, "E\t%s", k)
		for _, e := range data.ExtensionsByLanguage[k] {
			fmt.Fprintf(f, "\t%s", e)
		}
		fmt.Fprintln(f)
	}
	for _, k := range sortedFilenameKeys() {
		fmt.Fprintf(f, "F\t%s", k)
		for _, l := range data.LanguagesByFilename[k] {
			fmt.Fprintf(f, "\t%s", l)
		}
		fmt.Fprintln(f)
	}
	return f.Close()
}

// writeTypes emits `<language>\t<type>` with the type as enry's integer code
// (0 unknown, 1 data, 2 programming, 3 markup, 4 prose).
func writeTypes(path string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()

	for _, k := range sortedTypeKeys() {
		fmt.Fprintf(f, "%s\t%d\n", k, data.LanguagesType[k])
	}
	return f.Close()
}

// The enry module is go1.12 (no generics), so each map gets its own sort.

func sortedAliasKeys() []string {
	keys := make([]string, 0, len(data.LanguageByAliasMap))
	for k := range data.LanguageByAliasMap {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func sortedExtensionKeys() []string {
	keys := make([]string, 0, len(data.ExtensionsByLanguage))
	for k := range data.ExtensionsByLanguage {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func sortedFilenameKeys() []string {
	keys := make([]string, 0, len(data.LanguagesByFilename))
	for k := range data.LanguagesByFilename {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func sortedTypeKeys() []string {
	keys := make([]string, 0, len(data.LanguagesType))
	for k := range data.LanguagesType {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

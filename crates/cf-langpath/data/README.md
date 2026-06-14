# Vendored enry data (`enry-v2.1.0.tsv`)

This is a verbatim, machine-extracted snapshot of the three Linguist data tables
that `github.com/src-d/enry/v2 v2.1.0` generates and embeds in its `data`
package. It is the **same** data the reference `codefang` binary links, vendored
here so `cf-langpath`'s language→glob classification is byte-for-byte identical
(DESIGN §2.6). Nothing in this file is hand-written.

## Tables captured

| Tag | enry source                 | Meaning                                  |
| --- | --------------------------- | ---------------------------------------- |
| `A` | `data.LanguageByAliasMap`   | `alias_key` → canonical Linguist name    |
| `E` | `data.ExtensionsByLanguage` | canonical name → extensions (with dot)   |
| `F` | `data.LanguagesByFilename`  | literal filename → canonical name(s)     |

`enry.GetLanguageByAlias` reads `data.LanguageByAliasMap`;
`enry.GetLanguageExtensions` reads `data.ExtensionsByLanguage` directly.

Format: tab-separated, one record per line.

```
A<TAB><alias_key><TAB><canonical>
E<TAB><canonical><TAB><ext1><TAB><ext2>...
F<TAB><filename><TAB><lang1><TAB><lang2>...
```

`alias_key` is already enry's normalized form (substring before the first comma,
spaces→`_`, lowercased); the runtime applies the same normalization
(`convert_to_alias_key`) to incoming tokens before lookup.

Cardinalities (enry v2.1.0): **750** aliases, **504** extension-languages,
**234** filename records (1488 lines total, 32053 bytes, pure ASCII). The
`vendored_tables_loaded` unit test pins all three counts.

## Regenerating

The canonical, committed dumper lives at `tools/enrydump/main.go`. Run it from
the Go module root (so enry resolves from `go.sum`):

```sh
go run ./tools/enrydump crates/cf-langpath/data/enry-v2.1.0.tsv
```

It prints `aliases=750 extensions=504 filenames=234`. The dumper sorts every
section and each map's keys, so its output is byte-stable across runs. If enry is
upgraded, regenerate, bump the `include_str!` path in `src/lib.rs`, and update
the cardinality assertions in the `vendored_tables_loaded` test. The dumper
source, for reference:

```go
package main

import (
	"fmt"
	"os"
	"sort"

	"github.com/src-d/enry/v2/data"
)

func main() {
	f, _ := os.Create(os.Args[1])
	defer f.Close()

	keys := make([]string, 0, len(data.LanguageByAliasMap))
	for k := range data.LanguageByAliasMap {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	for _, k := range keys {
		fmt.Fprintf(f, "A\t%s\t%s\n", k, data.LanguageByAliasMap[k])
	}

	ek := make([]string, 0, len(data.ExtensionsByLanguage))
	for k := range data.ExtensionsByLanguage {
		ek = append(ek, k)
	}
	sort.Strings(ek)
	for _, k := range ek {
		fmt.Fprintf(f, "E\t%s", k)
		for _, e := range data.ExtensionsByLanguage[k] {
			fmt.Fprintf(f, "\t%s", e)
		}
		fmt.Fprintln(f)
	}

	fk := make([]string, 0, len(data.LanguagesByFilename))
	for k := range data.LanguagesByFilename {
		fk = append(fk, k)
	}
	sort.Strings(fk)
	for _, k := range fk {
		fmt.Fprintf(f, "F\t%s", k)
		for _, l := range data.LanguagesByFilename[k] {
			fmt.Fprintf(f, "\t%s", l)
		}
		fmt.Fprintln(f)
	}
}
```

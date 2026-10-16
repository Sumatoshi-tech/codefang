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

## Language types (`enry-types-v2.1.0.tsv`)

enry's `GetLanguageType` table, `data.LanguagesType`: one `language<TAB>code` row
per Linguist language, where the code is enry's `Type` enum (1 data, 2
programming, 3 markup, 4 prose; `unknown` is never stored). It is loaded by
`src/linguist_type.rs`, which exposes `language_type(name)`. Names are enry's
exact canonical names (`JSON`, `Go`), not the UAST's lowercase language names.

## Regenerating

The canonical, committed dumper lives at `tools/enrydump/main.go`. Run it from
the Go module root (so enry resolves from `go.sum`):

```sh
go run ./tools/enrydump crates/cf-langpath/data/enry-v2.1.0.tsv crates/cf-langpath/data/enry-types-v2.1.0.tsv
```

Run it from the enry v2.1.0 module (`git clone --branch v2.1.0
https://github.com/src-d/enry`), with the dumper copied to `tools/enrydump` in that
module, so the import resolves to the pinned data. The first file is
byte-identical to the vendored lookup table, and the second has 519 type rows.
It prints `aliases=750 extensions=504 filenames=234`. The dumper sorts every
section and each map's keys, so its output is byte-stable across runs. If enry is
upgraded, regenerate, bump the `include_str!` path in `src/lib.rs`, and update
the cardinality assertions in the `vendored_tables_loaded` test. The dumper
source is `tools/enrydump/main.go`, committed in this repository. The
lookup table was checked against it byte for byte.

# Troubleshooting empty Kotlin analysis

Kotlin `.kt` sources and `.kts` scripts are supported. Check a source file
containing a function directly:

```sh
uast parse path/to/Calculator.kt --format json
codefang run -a static/complexity --format json --per-file /path/to/repo
```

If `uast` reports `no tree-sitter grammar wired for language kotlin`, rebuild
both tools from the current checkout:

```sh
make install
command -v codefang
command -v uast
```

Ensure the resolved paths point to the newly installed tools. A reproduced case
with an older installed build reported zero functions for Spring Petclinic
Kotlin; the current checkout reported 96. See the
[investigation](../specs/bugs/BUG-kotlin-empty-analysis.md) for pinned inputs.

For files intentionally excluded by generated or vendor rules, inspect the
applicable flags using `codefang run --help`. Those filters are separate from
grammar availability.

The published [installation guide](../site/getting-started/installation.md)
also includes this diagnostic.

## Empty Shotness history

Test the history analyzer separately from static complexity:

```sh
codefang run -a history/shotness --limit 200 --format text /path/to/repo
```

An older Kotlin mapping can parse functions successfully while omitting
`props.name`, which the default Shotness query needs. This produces zero nodes
even when static complexity finds functions. The mapping now captures Kotlin
function, class and object names, including named and implicit companion
objects. For example, methods are tracked as `Service.work`,
`Service.Companion.work` or `Service.Factory.work`.

Rebuild both tools using `make install` after updating the source. The same
200-commit command on Spring Petclinic Kotlin changed from zero nodes to
14 nodes, 21 changes and 29 couplings after this fix. See the
[Shotness investigation](../specs/bugs/BUG-kotlin-shotness-empty.md).

For containerized runs, rebuild and redeploy the image from the updated source
so the container includes the corrected Kotlin mapping.

The [cross-language audit](shotness-languages.md) records the related fixes and
coverage for other language mappings.

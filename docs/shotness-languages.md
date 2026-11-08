# Shotness language coverage

Shotness needs a declaration name and a span covering the declaration's body.
A successful parse or a nonempty static complexity report does not establish
that history analysis can identify the same declaration.

Run the history path directly:

```sh
codefang run -a history/shotness --limit 200 --format text /path/to/repo
```

Use `--format json` to inspect `node_hotness` names and change counts. An empty
commit window or changes limited to function-free files can legitimately
produce zero entities.

## Verified mappings

The regression matrix covers representative named declarations in all 42
filename-selected mappings that declare functions, methods or structural entry
points. It uses real Git fixtures and both text and JSON output. Body-edit
fixtures introduce declarations, then change their bodies in a second commit;
every named entity must have the correct name and two touches.

| Result | Languages |
|---|---|
| Naming, selection or span gaps repaired in this audit | Clojure, CMake, Common Lisp, Elixir, Elm, Fortran, GraphQL, Groovy, LaTeX, Make, PowerShell, Protocol Buffers, R, Rego, SQL |
| Representative cases passed before these repairs | Bash, C, C++, C#, Crystal, Dart, Fish, Go, Go templates, Haskell, Java, JavaScript, Kotlin, Lua, Nim, Perl, PHP, PRQL, Python, Ruby, Rust, Scala, Swift, Tcl, TypeScript, TSX, Zig |

Additional cases cover private definitions, macros, Elixir guards, class
qualification, anonymous values and all five R assignment operators. A separate
check rejects fixtures with parser errors. A registry-coverage test requires a
history fixture when another function-declaring mapping becomes selectable.

The shared matcher now roots captures at the current CST node, so an identifier
or literal inside a nested expression cannot become its parent's name. Explicit
mapping names take precedence over the grammar's raw `name` field, allowing
wrapped names such as LaTeX environment names to be normalized.

## Limits

CSS function references are tested as references, with zero declaration entities.
The other mappings without function declarations likewise have no default
Shotness entities. Helm's overlapping YAML extensions currently select YAML;
the `rust_with_rstml` mapping has no filename extension. Those two registry
selection limits are asserted separately from name extraction.
The matrix uses registered extensions; conventional names such as `Makefile`
and `CMakeLists.txt` are outside this parser's extension-based selection.

The pinned LaTeX grammar exposes only the header of an old-style `\def` command.
Its name and introduction are tracked; body-only edits on later lines cannot
be attributed to that header. New-style commands, environments, theorems
and paired delimiters have body-edit coverage. These fixtures do not establish
coverage of every dialect, overload or multi-clause definition. Rust `impl`
method qualification retains the limit described in the
[Shotness reference](../site/analyzers/shotness.md).

After updating the source, rebuild the binaries with `make install`. For a
containerized deployment, rebuild and redeploy the image. See the
[investigation](../specs/bugs/BUG-shotness-language-names.md) and
[CLI regression matrix](../bins/codefang/tests/shotness_languages_e2e.rs).

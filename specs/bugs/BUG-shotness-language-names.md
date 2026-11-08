# BUG-shotness-language-names: language mappings drop Shotness entities

## Summary

A cross-language audit after the Kotlin fix finds additional named declarations
that parse successfully but produce missing or incorrect Shotness identities,
and declaration headers that exclude body edits from tracked spans.

## Reproduction

- Test: `bins/codefang/tests/shotness_languages_e2e.rs`.
- Command: `cargo test -p codefang --test shotness_languages_e2e`.
- Body-edit fixtures create real Git histories with two commits: introduce a
  named declaration, then change its body from `111` to `222`. Reference-only
  fixtures assert zero entities; the old LaTeX header has an introduction test.
- The real CLI runs `history/shotness --limit 200` in both text and JSON formats.
- Expected: the declared name, one tracked entity per declaration, two touches.
- Initial failures include Clojure, CMake, Common Lisp, Elixir, Elm, Fortran,
  GraphQL, Groovy, Make, PowerShell, Protocol Buffers, R, Rego and SQL.
  The compiled-registry extension also reproduces missing/wrapped LaTeX names.
- Raw parser audit: `/tmp/codefang-shotness-languages/audit-before.txt`.
- CLI regression capture: `/tmp/codefang-shotness-languages/cli-before-full.log`.

## Expected Behavior

The default Shotness query finds named declarations and counts edits throughout
their declaration spans. Anonymous values are excluded. Language keywords,
parameters and identifiers inside a body are not declaration names.

## Actual Behavior and Root Cause

Shotness uses the `Function` role and `props.name`. Several mapping tables expose
neither the grammar's actual name location nor the complete declaration span.
Examples include defining forms with positional names (Clojure), names inside
headers (CMake, Common Lisp, Fortran), non-`name` fields (Groovy), and name-wrapper
nodes (GraphQL, PowerShell, LaTeX). Protocol Buffer RPC methods lack the `Function`
role used by the default query. R exposes the keyword `function` as a grammar
field called `name`, which is not the variable receiving a function value.

Two shared defects also affect name extraction. Query cursors searched nested
expressions before matching the current node: an R right-assignment captured
the literal `111` inside a function instead of its target `probe`. Root-bounded
matching fixes this while retaining descendant captures explicitly present in
the pattern. Separately, grammar-derived `name` fields overwrote explicit mapping
properties, replacing LaTeX's normalized `probe` with `{probe}`. Explicit mapping
names now take precedence over the grammar-field fallback.

## Fix and Acceptance

Use explicit language mappings for actual declaration names and full spans.
Preserve the default Shotness DSL and anonymous-value exclusion. Keep headers
structural when their enclosing node represents the function.

The audit covers representative named declarations in all 42 filename-selected
language mappings that model functions, methods or structural entry points,
plus a CSS reference-only negative case. A registry-derived test requires a
history fixture whenever another function-declaring mapping becomes selectable.
The fixtures cover private definitions, macros, class qualification, guarded
Elixir functions, all five R assignment operators, and anonymous-value exclusion.
R right-assigned function values are parenthesized to preserve their grammar
precedence. Static complexity still measures the named R function body.

## Audit Boundaries

- The remaining 24 mappings do not declare functions/methods, including CSS
  function references. Plain data files have no default Shotness entities.
- Helm's overlapping `.yaml`/`.yml` extensions select YAML, and `rust_with_rstml`
  has no registered filename extension. These two selection limits are asserted
  by the registry-coverage test; changing either requires revisiting the matrix.
- The pinned LaTeX grammar models only the header of old-style `\def` commands:
  [grammar source](https://github.com/latex-lsp/tree-sitter-latex/blob/f736d24d89acbd90092d92089e5171e6a449db40/grammar.js).
  Their names and introduction are tested; body-only edits on later lines
  cannot be attributed to that header. New-style commands,
  environments, theorems and paired delimiters have body-edit coverage.
- This audit does not assert every dialect, overload or multi-clause form.
  It uses registered extensions; conventional `Makefile`/`CMakeLists.txt` names
  and the existing Rust `impl` qualification limit are outside this repair.

## Stressors

1. A grammar uses an anonymous keyword as its `name` field.
2. A declaration name is inside a header wrapper.
3. A header is counted as a second function.
4. A body edit occurs after the header line.
5. A defining macro uses positional names.
6. A nested declaration has a name before its parent gets one.
7. An anonymous function contains named references.
8. A custom Shotness name/selection query is configured.
9. A method lacks the default query's `Function` role.
10. Two languages share an extension.
11. A fixture uses syntax unsupported by the pinned grammar.
12. The executable on PATH predates the mapping changes.

## Traceability

- Evidence: [before/after captures](../runs/shotness-language-names/reproduction.md).
- Validation: [workspace gates and installed-binary verification](../runs/shotness-language-names/verification.md).
- User guidance: [coverage and limits](../../docs/shotness-languages.md).
- Regression fixtures: `bins/codefang/tests/fixtures/shotness_languages/`.
- CLI matrix: `bins/codefang/tests/shotness_languages_e2e.rs`.
- Mapping tables: `crates/cf-uast-mappings/src/`.
- Root matching: `crates/cf-uast-mapping/src/pattern_matcher.rs` and
  `crates/cf-uast/tests/lowering_contract.rs`.
- Explicit-name precedence: `crates/cf-uast/src/lowering.rs`.
- Valid syntax and registry coverage: `crates/cf-uast-mappings/tests/shotness_fixtures.rs`.
- Prior issue: [Kotlin Shotness](BUG-kotlin-shotness-empty.md).

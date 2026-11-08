# BUG-kotlin-empty-analysis: installed tools lack the Kotlin grammar

## Summary

The installed tools return empty Kotlin static complexity because their parser
build does not include the Kotlin grammar; the current checkout already includes it.

This investigation covered `static/complexity`. The screenshot's exact
`history/shotness --limit 200 --format text` command exposed an additional bug in
the current source: missing declaration names. See
[BUG-kotlin-shotness-empty](BUG-kotlin-shotness-empty.md) for its reproduction
and mapping fix. Passing static complexity did not establish Shotness correctness.

## Reproduction

- Method: manual, using the real installed binaries and a public repository.
- Repository: [Spring Petclinic Kotlin](https://github.com/spring-petclinic/spring-petclinic-kotlin/commit/817dba3b02261e4291f138d5d8bb8c85c7d49758).
- Download: `curl -fL https://codeload.github.com/spring-petclinic/spring-petclinic-kotlin/tar.gz/817dba3b02261e4291f138d5d8bb8c85c7d49758 -o /tmp/petclinic.tar.gz`
- Extract: `tar -xzf /tmp/petclinic.tar.gz -C /tmp`
- Command: `~/.cargo/bin/codefang run -a static/complexity --format json --per-file /tmp/spring-petclinic-kotlin-817dba3b02261e4291f138d5d8bb8c85c7d49758`
- Evidence: `Total Functions` is `0` in [installed-before.json](../runs/kotlin-empty-analysis/installed-before.json).
- Parser probe: `~/.cargo/bin/uast parse /tmp/codefang-kotlin-repro/Calculator.kt --format json`
- Parser error: `no tree-sitter grammar wired for language kotlin (grammar vendoring pending; see crate docs)`.
- Control: the freshly built checkout analyzes the same repository and reports
  96 functions, cyclomatic total 120 and maximum 5; see
  [checkout-result.json](../runs/kotlin-empty-analysis/checkout-result.json).

## Expected Behavior

Kotlin source files (`.kt`) and scripts (`.kts`) produce function metrics when
analyzed as files or through a directory walk. The parser supports Kotlin.

## Actual Behavior

The installed analyzer returns success with zero functions. Its installed UAST
tool explicitly reports a missing grammar for a nonempty Kotlin class.

## Root Cause Analysis

The installed executables differ from the current source. Extension registration
alone does not guarantee grammar availability in an older build. The current
`cf-uast/build.rs` compiles Kotlin's vendored parser and external scanner;
`cf-uast/src/languages.rs` resolves `kotlin` to `tree_sitter_kotlin`. Both are
already present, and the existing Kotlin lowering test passes without changes.
The installed parser rejects the same language, while the new CLI returns real
metrics on the public sample. No parser or analyzer source change is justified
by this reproduction.

## Fix

Build and install both binaries from the current checkout. Add a hermetic CLI
regression for a companion-object function, exercising `.kt`, `.kts`, individual
file analysis and a nested directory walk. The test asserts the function name
and counts, so extension-only support or an empty fallback cannot satisfy it.
The fixture follows the structure of the public
[Kotlin Calculator sample](https://github.com/Kotlin/kmp-basic-sample/blob/83e1d17070878ce1ded5936b789bff515aa63e7f/shared/src/commonMain/kotlin/com/jetbrains/basicsample/Calculator.kt).

Both binaries have been rebuilt and installed. The installed `codefang` now
reports 96 functions on the same public repository
([installed-after.json](../runs/kotlin-empty-analysis/installed-after.json));
the installed `uast` parses the companion-object fixture and yields one function
([parser-after.json](../runs/kotlin-empty-analysis/parser-after.json)).
Binary hashes and reproduction details are retained in the
[evidence directory](../runs/kotlin-empty-analysis/reproduction.md).

## Stressors and Validation

1. Stale executables: compare installed and freshly built results.
2. Extension-only registration: assert nonzero function metrics.
3. Missing parser grammar: probe `uast parse` directly.
4. Missing external scanner: parse the real Kotlin fixture.
5. `.kt` source: analyze it as a file.
6. `.kts` script: analyze it as a file.
7. Nested Gradle source paths: walk `src/main/kotlin`.
8. Companion-object nesting: assert discovery of `sum`.
9. Expression-bodied functions: assert one function for `sum`.
10. Mixed source/script roots: assert two functions in two files.
11. Developer configuration leakage: use the existing isolated CLI test helper.
12. Upstream source changes: retain hermetic test data and pin reproduction links.

## Traceability

- Verification: [workspace tests, lint, dead code and installed-tool checks](../runs/kotlin-empty-analysis/verification.md).
- Regression: `bins/codefang/tests/static_walk_e2e.rs:kotlin_sources_and_scripts_produce_function_metrics`.
- Existing grammar wiring: `crates/cf-uast/build.rs`, `crates/cf-uast/src/languages.rs`.
- Operational fix: rebuild and replace `~/.cargo/bin/codefang` and `~/.cargo/bin/uast`.
- User guidance: [Kotlin analysis troubleshooting](../../docs/kotlin-analysis.md).
- Roadmap: [Kotlin analysis repair](ROADMAP.md).

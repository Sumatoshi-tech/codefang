# BUG-kotlin-shotness-empty: Kotlin functions have no Shotness names

## Summary

`history/shotness --limit 200 --format text` reports zero nodes for Kotlin
functions even when the Kotlin grammar is available.

## Reproduction

- Test: `bins/codefang/tests/shotness_e2e.rs:kotlin_shotness_with_limit_and_text_reports_functions`.
- Command: `cargo test -p codefang --test shotness_e2e kotlin_shotness_with_limit_and_text_reports_functions -- --nocapture`.
- Fixture: a real Git commit containing `fun sum(a: Int, b: Int): Int = a + b`.
- Actual CLI arguments: `run -a history/shotness --limit 200 --format text <fixture>`.
- Failure: the CLI exits successfully but prints `0 nodes`, `Total Nodes 0`
  and `Total Changes 0`; the assertion requiring `1 nodes` fails.
- Captured output: `/tmp/codefang-kotlin-repro/shotness-red.log`.
- Public reproduction: [Spring Petclinic Kotlin](https://github.com/spring-petclinic/spring-petclinic-kotlin/commit/817dba3b02261e4291f138d5d8bb8c85c7d49758),
  cloned with real commit history. The exact command also yielded zero nodes.
- [Persistent before/after reports](../runs/kotlin-shotness-empty/reproduction.md).

## Expected Behavior

Kotlin functions are named tracked entities, with their enclosing declarations
qualifying their names. Nonempty Kotlin history produces Shotness metrics.

## Actual Behavior

Functions are parsed but discarded before Shotness tracking because the default
name query yields no name.

## Root Cause Analysis

Shotness uses `.props.name` to identify selected declarations. Kotlin's grammar
does not provide a `name` field for its function declarations. The generic
lowering fills `props.name` only from that field, and Kotlin's mapping does not
capture its direct `simple_identifier` as a name. Static complexity finds the
identifier independently, so the earlier static regression could pass while
Shotness remained empty. Reinstalling the grammar resolves the separate stale
binary problem but does not resolve this defect in the current source.

## Fix Scope and Validation

Capture declaration names explicitly in the Kotlin mapping. Keep Shotness's
name-query contract unchanged. Verify through the exact text/limit flags,
machine reports, qualified names, source/script extensions and a public Kotlin
repository with real commit history.

The function query captures the direct `simple_identifier`; class and object
queries capture their direct `type_identifier`. Named companion objects capture
their identifier before an unconditional fallback supplies Kotlin's implicit
`Companion` name. A condition on the direct `type_identifier` prevents unnamed
companions from adopting a nested named companion's identifier: query matching
searches the subtree, so testing the captured name alone is insufficient.
No Shotness query,
selection flag or reporting behavior is changed.

The public 200-commit text report changes from zero nodes to 14 nodes,
21 changes and 29 couplings. The test matrix covers an exact text/limit invocation,
`Calculator.sum`, `Service.work`, `Service.Companion.work`,
`Service.Factory.work`, two-commit `.kts` body changes and co-change counts,
plus annotated generic extension functions with unrelated identifiers.
A nested-companion regression asserts the class-name sequence
`Outer`, `Companion`, `Inner`, `Factory`.

Both corrected executables are installed. The installed text and JSON reports
agree on all three public-repository totals; the installed parser now exposes
`Calculator`, `Companion` and `sum` as declaration names for the earlier
companion-object sample. Captures are stored in
[the evidence directory](../runs/kotlin-shotness-empty/reproduction.md).

Stressors: missing grammar; missing name properties; unnamed lambdas; annotations
containing identifiers; extension receivers; generic type parameters; nested
classes; companion objects; `.kts` scripts; edited function bodies; custom
Shotness queries; format disagreement. Use direct-child captures to avoid
accidentally naming declarations after annotations, receivers or body references.

## Traceability

- Verification: [workspace suite and installed-tool results](../runs/kotlin-shotness-empty/verification.md).
- Regression: `bins/codefang/tests/shotness_e2e.rs`.
- Mapping: `crates/cf-uast-mappings/src/kotlin.rs`.
- Mapping regression: `crates/cf-uast-mappings/tests/shape_contract.rs:kotlin_names_exclude_annotation_receiver_and_body_identifiers`.
- Usage: [Kotlin troubleshooting](../../docs/kotlin-analysis.md) and
  [Shotness reference](../../site/analyzers/shotness.md).
- Related static-only investigation: [BUG-kotlin-empty-analysis](BUG-kotlin-empty-analysis.md).
- Roadmap: [Kotlin Shotness repair](ROADMAP.md).

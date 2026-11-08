# Bug repair roadmap

## Kotlin analysis repair

- [x] Reproduce empty results on a public Kotlin project and identify the cause:
  [BUG-kotlin-empty-analysis](BUG-kotlin-empty-analysis.md).
- [x] Add CLI regression coverage for Kotlin sources, scripts and nested walks:
  [static_walk_e2e.rs](../../bins/codefang/tests/static_walk_e2e.rs).
- [x] Verify workspace tests, lint and dead code checks:
  [verification results](../runs/kotlin-empty-analysis/verification.md).
- [x] Replace the installed binaries and verify the public project again:
  [before/after evidence](../runs/kotlin-empty-analysis/reproduction.md).
- [x] Document [usage and troubleshooting](../../docs/kotlin-analysis.md).

## Kotlin Shotness repair

- [x] Reproduce the screenshot's exact flags on a real Kotlin Git fixture and
  public repository: [BUG-kotlin-shotness-empty](BUG-kotlin-shotness-empty.md).
- [x] Capture function and container names in the
  [Kotlin mapping](../../crates/cf-uast-mappings/src/kotlin.rs).
- [x] Cover text/limit flags, qualified names and script change/coupling metrics
  in [Shotness CLI tests](../../bins/codefang/tests/shotness_e2e.rs).
- [x] Cover annotation, receiver and body identifiers in the
  [mapping shape test](../../crates/cf-uast-mappings/tests/shape_contract.rs).
- [x] Verify workspace tests, lint and dead code checks:
  [final verification](../runs/kotlin-shotness-empty/verification.md).
- [x] Install corrected binaries and repeat the public repository command:
  [installed text report](../runs/kotlin-shotness-empty/public-after.txt) and
  [matching JSON totals](../runs/kotlin-shotness-empty/public-after-aggregate.json).
- [x] Correct the earlier static-only conclusion and document Kotlin Shotness.

## Shotness language-name audit

- [x] Reproduce naming, selection and body-span gaps across other mappings:
  [BUG-shotness-language-names](BUG-shotness-language-names.md).
- [x] Repair the 15 affected language tables and shared capture/name precedence.
- [x] Add a real-history [language matrix](../../bins/codefang/tests/shotness_languages_e2e.rs),
  parser-validity checks and a registry-derived coverage gate.
- [x] Document [coverage and parser limits](../../docs/shotness-languages.md).
- [x] Verify workspace tests, lint and dead code checks:
  [verification](../runs/shotness-language-names/verification.md).
- [x] Rebuild installed binaries and confirm every mixed-repository name/count:
  [installed results](../runs/shotness-language-names/installed-polyglot-summary.json).

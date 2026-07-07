# cf-budget

Memory-budget auto-tuning for codefang analysis runs.

Given a single memory-budget value (bytes) it derives:

- `solve_for_budget` → a history `CoordinatorConfig` (workers, blob/diff caches,
  buffer size, blob arena).
- `solve_static_budget` → a `StaticBudgetConfig` (worker cap, spill threshold)
  for the static analysis phase.
- `native_limits_for_budget` → libgit2 native memory limits (`mwindow` mapped
  limit, object cache size, `MALLOC_ARENA_MAX`).

This crate emits configuration structs only — it never serializes a
machine-format report — so it does not depend on the cf-gojson/cf-goyaml
byte-identity encoders. Its integer/float truncation arithmetic is
reference-implementation behavior: the derived knobs are bit-identical for a
given CPU count.

See `specs/rust-rewrite/DESIGN.md`.

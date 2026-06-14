# cf-uast-spec

Embeds the canonical UAST artifacts at compile time:

- `uast-schema.json` — the JSON Schema used as the built-in default by
  `uast validate`.
- `uast-example.json` — a reference UAST document.

Both files are included **verbatim** via `include_str!`, so the bytes this crate
serves are the canonical schema/example bytes with no reformatting,
re-serialization, or whitespace normalization (CLI compatibility contract). The
crate performs no report serialization and therefore does not depend on the
report-format serialization crates (`cf-gojson` / `cf-goyaml`) — it only hands
back the canonical bytes.

## API

```rust
let schema: &str     = cf_uast_spec::schema();        // == cf_uast_spec::SCHEMA
let schema_b: &[u8]  = cf_uast_spec::schema_bytes();
let example: &str    = cf_uast_spec::example();        // == cf_uast_spec::EXAMPLE
let example_b: &[u8] = cf_uast_spec::example_bytes();

// Path-addressable lookup by logical file name:
let bytes = cf_uast_spec::read_file(cf_uast_spec::SCHEMA_FILE_NAME);
```

## Updating the embedded data

The two JSON files live in `src/` and are intentionally not transformed in any
way. If the canonical schema or example changes, replace the files in `src/`
with the new canonical copies; the embed picks them up at the next build.

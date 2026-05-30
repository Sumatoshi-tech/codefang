# cf-uast-spec

Rust port of the Go package `pkg/uast/pkg/spec`.

This crate embeds the canonical UAST artifacts at compile time:

- `uast-schema.json` — the JSON Schema used as the built-in default by
  `uast validate`.
- `uast-example.json` — a reference UAST document.

Both files are included **verbatim** (copied from the Go source tree and embedded
via `include_str!`), so the bytes this crate serves are byte-identical to the
bytes the Go binary embeds with `//go:embed`. The crate performs no report
serialization and therefore does not depend on the go-compat serialization crates
(`cf-gojson` / `cf-goyaml`) — it only hands back the canonical bytes.

The Go original (`schemafs.go`) is:

```go
//go:embed uast-schema.json
var UASTSchemaFS embed.FS
```

and its only consumer, `cmd/uast/validate.go`, reads the schema with
`spec.UASTSchemaFS.ReadFile("uast-schema.json")`. The [`read_file`] shim
reproduces that lookup.

## API

```rust
let schema: &str     = cf_uast_spec::schema();        // == cf_uast_spec::SCHEMA
let schema_b: &[u8]  = cf_uast_spec::schema_bytes();
let example: &str    = cf_uast_spec::example();        // == cf_uast_spec::EXAMPLE
let example_b: &[u8] = cf_uast_spec::example_bytes();

// Path-addressable shim mirroring Go's embed.FS.ReadFile:
let bytes = cf_uast_spec::read_file(cf_uast_spec::SCHEMA_FILE_NAME);
```

## Regenerating the embedded data

The two JSON files are vendored copies of the upstream Go source files:

- `pkg/uast/pkg/spec/uast-schema.json`
- `pkg/uast/pkg/spec/uast-example.json`

If the upstream schema changes, copy the files again into `src/` to keep them in
sync (they are intentionally not transformed in any way), e.g.:

```sh
cp ../../../pkg/uast/pkg/spec/uast-schema.json  src/uast-schema.json
cp ../../../pkg/uast/pkg/spec/uast-example.json src/uast-example.json
```

[`read_file`]: https://docs.rs/cf-uast-spec

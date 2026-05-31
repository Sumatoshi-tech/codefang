# cf-reportutil

Rust port of the Go package `internal/analyzers/common/reportutil`.

Provides:

- **CFB1 binary envelope** — `[4-byte magic "CFB1"][LE u32 length][compact escaped JSON]`
  via `encode_binary` / `decode_binary`.
- **Scalar extraction** — `get_int` / `get_float64` over `BTreeMap<String, Value>`,
  matching Go's `GetInt` / `GetFloat64` over `map[string]any`.
- **Human formatting** — `format_report_text`, `merge_string_maps`.

## Byte identity

The CFB1 payload must match Go's `encoding/json.Marshal` byte-for-byte. The
in-crate `GoJson` marshaller reproduces Go's defaults: compact output,
HTML-escaping of `<` `>` `&`, and escaping of `U+2028` / `U+2029`. Use a
`BTreeMap<String, _>` to reproduce Go's map-key sorting, or a struct whose field
order matches the Go struct's declaration order.

Per `specs/rust-rewrite/DESIGN.md` (§2, §4), MACHINE-format serialization is
ultimately owned by the shared `go-compat` crate. `encode_binary` is generic
over the `JsonMarshal` trait (`encode_binary_with`) so the `go-compat`
implementation can be dropped in centrally once it is ported, without changing
call sites.

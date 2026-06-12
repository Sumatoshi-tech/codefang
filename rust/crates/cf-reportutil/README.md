# cf-reportutil

Shared helpers for analyzers that emit reports.

Provides:

- **CFB1 binary envelope** — `[4-byte magic "CFB1"][LE u32 length][compact escaped JSON]`
  via `encode_binary_envelope` / `decode_binary_envelope` / `decode_binary_envelopes`.
- **Typed accessors** — `get`, `get_int`, `get_float64`, `get_string`,
  `get_string_slice`, `get_functions`, `get_string_int_map`, `map_string` over
  the dynamic report map (`cf_gojson::GoMap`). Numeric coercion delegates to
  `cf-safeconv`; everything else is a strict type match.
- **Scalar formatting** — `format_int`, `format_float`, `format_percent`,
  `pct` for human-readable report fields.

## Byte identity

The CFB1 payload is the compact, HTML-escaped report-contract JSON encoding.
Per `specs/rust-rewrite/DESIGN.md` (§2.2, §2.5) all machine-format
serialization flows through the shared `cf-gojson` encoder — never
`serde_json` — so map keys byte-sort, `<` `>` `&` and `U+2028`/`U+2029`
escape, and no insignificant whitespace or trailing newline is emitted.

Compatibility: output bytes are pinned against the reference implementation by
the differential gate in `rust/tests/compat`. Error strings are part of the
CLI compatibility contract and are asserted by tests.

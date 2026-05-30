# cf-pathfilter

Rust port of Go `pkg/pathfilter`.

Path include/exclude filtering for code analysis: excludes vendor / third-party
files and generated files. Consumed by `pathpolicy`, `framework`, `plumbing`,
and `file_history`.

## Behaviour

A [`Filter`] combines three rule sets:

1. **Vendor detection** — a faithful reproduction of `src-d/enry` v2.1.0's
   `IsVendor` ([`is_vendor`]): the path is matched (unanchored, like Go's
   `regexp.MatchString`) against the Linguist-derived vendor regex table.
2. **Generated suffixes** — `.pb.go`, `.min.js`, `_string.go`, etc.
   ([`DEFAULT_SUFFIXES`]).
3. **Generated filename prefixes** — `zz_generated`, `mock_`, `fake_`,
   `wire_gen`, matched against the path base name ([`DEFAULT_FILENAME_PREFIXES`]).

Content-aware checks scan the first 512 bytes for generated-file markers
(`DO NOT EDIT`, `@generated`, ...).

| Method | Vendor | Suffix/prefix | Content markers |
| --- | --- | --- | --- |
| `is_excluded` | yes | yes | no |
| `is_excluded_with_content` | yes | yes | yes |
| `is_generated_path` | no | yes | no |
| `is_generated_content` | no | no | yes |

```rust
use cf_pathfilter::Filter;

let f = Filter::new();
assert!(f.is_excluded("vendor/github.com/foo/bar.go")); // vendor
assert!(f.is_excluded("api/types.pb.go"));              // generated suffix
assert!(!f.is_excluded("internal/server/handler.go")); // normal source
```

## Data parity

Per `specs/rust-rewrite/DESIGN.md` §2.6 and porting rule 7, file classification
changes *which* bytes appear in machine-format reports, so the enry vendor regex
table is vendored to match the Go library. `build.rs` extracts the literal regex
sources from the local enry v2.1.0 `data/vendor.go` (resolved via
`CF_ENRY_VENDOR_GO` or the Go module cache) and emits them as a Rust slice; an
in-tree best-effort fallback (`src/vendor_data.rs`) keeps offline builds working.
The Go regexp engine is RE2-syntax, matched by the `regex` crate, so identical
source strings yield identical match behaviour.

This crate emits no machine-format report bytes of its own — it is a pure
predicate — so it does **not** depend on `cf-gojson` / `cf-goyaml`.

## External crates

- `regex` — RE2-compatible matching of enry vendor patterns.
- `once_cell` — one-time compilation of the vendor matcher table.

# cf-version

Build/version metadata (`Version`, `Commit`, `Date`) and the one-line banner
both codefang binaries print:

```
<name> <Version> (commit: <Commit>, built: <Date>)
```

## Injection model (build script)

Per `specs/rust-rewrite/DESIGN.md` §2.8, values are injected through
`build.rs`, which reads env vars and re-exports them as `rustc-env` values
consumed by `option_env!`:

| Field   | Env (first non-empty wins)              | Default     |
| ------- | --------------------------------------- | ----------- |
| Version | `CF_VERSION`, `GIT_VERSION`             | crate version |
| Commit  | `CF_COMMIT`, `GIT_COMMIT`, `GIT_HASH`, the checkout's `HEAD` | `unknown` |
| Date    | `CF_DATE`, `SOURCE_DATE_EPOCH` (→RFC3339 UTC) | `unknown` |

A plain `cargo build` with no env reports the crate version, the commit `HEAD` points at (or `unknown` outside a git checkout), and an `unknown` build date.
`SOURCE_DATE_EPOCH` keeps the `built:` date reproducible so version goldens
are stable. The banner format is a CLI contract, pinned by the `codefang`
and `uast` end-to-end tests.

## Usage

The banner always has the same shape, whatever was injected:

```rust
let line = cf_version::banner("codefang");
assert!(line.starts_with(&format!("codefang {} (commit: ", cf_version::VERSION)));
assert!(line.ends_with(&format!(", built: {})", cf_version::DATE)));
```

Build with injected metadata:

```sh
CF_VERSION=1.2.3 CF_COMMIT=$(git rev-parse --short HEAD) \
  SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) \
  cargo build -p cf-version
```

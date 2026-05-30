# cf-version

Rust port of the Go package `pkg/version`. Holds build/version metadata
(`Version`, `Commit`, `Date`) and renders the one-line banner both codefang
binaries print:

```
<name> <Version> (commit: <Commit>, built: <Date>)
```

## Injection model (ldflags → build script)

Go injects these strings at link time via `-ldflags "-X .../pkg/version.Version=..."`
and falls back to `dev` / `none` / `unknown`. Rust has no ldflags, so per
`specs/rust-rewrite/DESIGN.md` §2.8 we inject through `build.rs`, which reads env
vars and re-exports them as `rustc-env` values consumed by `option_env!`:

| Field   | Env (first non-empty wins)              | Default     |
| ------- | --------------------------------------- | ----------- |
| Version | `CF_VERSION`, `GIT_VERSION`             | `dev`       |
| Commit  | `CF_COMMIT`, `GIT_COMMIT`               | `none`      |
| Date    | `CF_DATE`, `SOURCE_DATE_EPOCH` (→RFC3339 UTC) | `unknown` |

A plain `cargo build` with no env produces `dev` / `none` / `unknown`,
byte-identical to a Go build with no ldflags. `SOURCE_DATE_EPOCH` keeps the
`built:` date reproducible so version goldens are stable on both sides.

## Usage

```rust
println!("{}", cf_version::banner("codefang"));
// codefang dev (commit: none, built: unknown)   (with no injection)
```

Build with injected metadata:

```sh
CF_VERSION=1.2.3 CF_COMMIT=$(git rev-parse --short HEAD) \
  SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) \
  cargo build -p cf-version
```

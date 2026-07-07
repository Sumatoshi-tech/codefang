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
| Version | `CF_VERSION`, `GIT_VERSION`             | `dev`       |
| Commit  | `CF_COMMIT`, `GIT_COMMIT`               | `none`      |
| Date    | `CF_DATE`, `SOURCE_DATE_EPOCH` (→RFC3339 UTC) | `unknown` |

A plain `cargo build` with no env produces `dev` / `none` / `unknown`.
`SOURCE_DATE_EPOCH` keeps the `built:` date reproducible so version goldens
are stable. The banner format is a frozen CLI contract, pinned against the
reference binary by `tests/compat`.

## Usage

With no build-time injection the defaults (`dev` / `none` / `unknown`) are used.
This exact behavior is pinned by the crate doctest and unit tests on
[`banner`](src/lib.rs):

```rust
assert_eq!(
    cf_version::banner("codefang"),
    "codefang dev (commit: none, built: unknown)",
);
```

Build with injected metadata:

```sh
CF_VERSION=1.2.3 CF_COMMIT=$(git rev-parse --short HEAD) \
  SOURCE_DATE_EPOCH=$(git log -1 --format=%ct) \
  cargo build -p cf-version
```

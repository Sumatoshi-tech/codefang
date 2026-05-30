<!-- Template: GitHub community standards (https://docs.github.com/en/communities/setting-up-your-project-for-healthy-contributions/about-community-profiles-for-public-repositories). Authored by /documenter contributing. -->

# Contributing to Codefang

Thanks for your interest in contributing. Whether you are fixing a bug, adding a
new analyzer, improving documentation, or proposing a feature, every
contribution makes Codefang better for the whole community.

## Read the full guide

The complete contributing guide is maintained on the documentation site:

**<https://sumatoshi-tech.github.io/codefang/contributing/>**

It covers everything you need to get started:

- **Getting started** — prerequisites, fork, clone, and build.
- **Development workflow** — branch, write tests first, lint, run the race detector.
- **Code standards** — idiomatic Go, context propagation, structured logging, error handling.
- **Commit conventions** — [Conventional Commits](https://www.conventionalcommits.org/).
- **Pull request process** — the pre-PR checklist and the PR description template.
- **Reporting bugs** and **feature requests** — what to include so maintainers can act.

The source for that page lives at
[`site/contributing/index.md`](site/contributing/index.md), which is the
canonical version. Edit it there.

## Quick reference

To build, test, and lint locally:

```bash
make build   # build all binaries (compiles vendored libgit2)
make test    # run the full test suite
make lint    # run golangci-lint and dead code analysis
```

## Versioning

Codefang follows [Semantic Versioning](https://semver.org/) (`MAJOR.MINOR.PATCH`).
A release is cut by the GoReleaser pipeline in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml), which runs on any pushed
`v*` tag (the workflow's `tags: ["v*"]` trigger), so the tag you choose is the
contract you ship.

- **MAJOR** — a breaking change to the public surface (see below).
- **MINOR** — a backward-compatible, additive feature.
- **PATCH** — a bug fix, performance improvement, or internal refactor that does
  not change the public surface.

The **public surface** is the exported Go API (anything reachable through
`pkg/`), the CLI flags and commands, the output schemas of those commands, and
the configuration keys; changing or removing any of these in an incompatible way
is a breaking change and requires a MAJOR bump.

## Contribution agreement

Codefang has **no separate Contributor License Agreement (CLA)** and **does not
require a Developer Certificate of Origin (DCO) sign-off** (`git commit -s`) on
commits. There is no CLA bot or DCO check in the
[`.github/workflows/`](.github/workflows/) pipelines, and none is planned.

Instead, contributions are accepted on an **inbound=outbound** basis under the
project's [Apache License 2.0](LICENSE). This follows the Apache-2.0
contribution clause (§5): unless you state otherwise, any contribution you
intentionally submit for inclusion is provided under the same Apache-2.0 terms
that cover the project, with no additional conditions. By opening a pull
request, you confirm you have the right to submit the work under that license.

If a maintainer ever adopts a CLA or starts requiring DCO sign-off, this section
and the relevant pipeline will be updated to say so.

## Code of conduct

This project follows a [Code of Conduct](https://sumatoshi-tech.github.io/codefang/contributing/code-of-conduct/).
By participating, you agree to uphold it.

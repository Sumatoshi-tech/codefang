<!-- Authored by /documenter release-notes (Good Docs Project release-notes template). Driver: /march Item 12 (R-RELEASE-04). -->

# Release notes

Release notes summarize what changed in each Codefang release, how to upgrade, and any known issues. They expand on the entries in the canonical [changelog](../contributing/changelog.md), which follows [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## How releases are cut

Codefang releases are automated. Pushing a `v*` tag (for example `v1.0.0`) triggers the GoReleaser pipeline in the [CI/CD workflow](https://github.com/Sumatoshi-tech/codefang/blob/main/.github/workflows/ci.yml), which builds the cross-platform binaries and publishes a [GitHub Release](https://github.com/Sumatoshi-tech/codefang/releases). Each release page here mirrors the GitHub Release and the matching changelog section.

## Releases

| Release | Notes |
|---|---|
| Upcoming release | [Release notes](unreleased.md) |

!!! note "No tagged release yet"
    Codefang has not cut its first tagged release. The [upcoming release](unreleased.md) page tracks everything staged in the `[Unreleased]` section of the changelog. Once the first `v*` tag ships, this page gains a dated row per release.

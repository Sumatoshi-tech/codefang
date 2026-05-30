<!-- Template: GOVERNANCE (https://chaoss.community/kb/metrics-model-oss-project-viability-governance/). Authored by /documenter governance. -->

# Governance

This document describes who decides what in Codefang, how those decisions are
made, and how the set of maintainers changes over time. It is an honest
description of the project as it stands: Codefang is led by a single
benevolent maintainer, with a clear path to growing the maintainer team as the
community grows.

## Project leadership

Codefang follows a benevolent-maintainer model. One maintainer holds final
authority over the project's direction, the code that ships, and the releases
that are cut. As of this writing, that maintainer is:

- Dmytro Gajewski (`d.y.gaevskiy@gmail.com`)

The current maintainer list is always the source of truth in the
[`MAINTAINERS`](MAINTAINERS) file at the repository root. When that file and
this document disagree, `MAINTAINERS` wins, and the disagreement is a bug to
fix here.

## Roles

- **Maintainer.** Has write access to the repository. Reviews and merges pull
  requests, cuts releases, sets the roadmap, and has the final say on disputes.
  Maintainers are listed in [`MAINTAINERS`](MAINTAINERS).
- **Contributor.** Anyone who has had a pull request merged. Contributors propose
  changes, file issues, review other contributions, and help triage. No special
  access is required to contribute.

There is no separate committee or steering group. While the project has a single
maintainer, "the maintainers" and "the maintainer" mean the same person.

## Decision process

The maintainer is the decision-maker. The process below is what to expect in
practice, scaled to the size of the change.

- **Small changes** (bug fixes, documentation, internal refactors that do not
  change the public surface): the maintainer reviews the pull request and merges
  it once it passes review and the checks in [`CONTRIBUTING.md`](CONTRIBUTING.md)
  are green.
- **Significant changes** (new public Go API, breaking changes, new CLI flags or
  output schemas, new dependencies): open an issue first to discuss the change
  before opening a pull request. The maintainer decides whether to accept the
  direction, then reviews the implementation. Record the rationale for a
  notable decision as an ADR under [`docs/adr/`](docs/adr/).
- **Releases:** the maintainer decides when to cut a release and which version to
  tag, following the Semantic Versioning policy documented in
  [`CONTRIBUTING.md`](CONTRIBUTING.md).

All change proposals go through a pull request and a review. The maintainer does
not merge their own substantial changes without giving the community a chance to
comment through the issue and pull-request process.

## Disputes

If you disagree with a decision, open an issue (or comment on the relevant pull
request) and make your case with evidence. The maintainer will respond and,
where reasonable, revisit the decision. While the project has a single
maintainer, that maintainer resolves disputes; once there is more than one
maintainer, disputes that cannot be settled by discussion are resolved by a
simple majority vote of the maintainers, with the longest-serving maintainer
breaking a tie.

## Adding a maintainer

The project actively wants to grow its maintainer team. A contributor becomes a
candidate for maintainer through:

1. **Sustained contribution.** A track record of merged pull requests, helpful
   reviews, and issue triage that shows good judgment about the codebase.
2. **Nomination.** An existing maintainer nominates the candidate by opening an
   issue or pull request that proposes adding them to [`MAINTAINERS`](MAINTAINERS).
3. **Consensus.** The existing maintainers agree. While there is a single
   maintainer, that maintainer's agreement is sufficient. Once there are several,
   adding a maintainer uses lazy consensus: the nomination is approved unless an
   existing maintainer objects with a reason within a reasonable review window.

A new maintainer is added by a pull request that updates [`MAINTAINERS`](MAINTAINERS)
and grants repository write access.

## Removing a maintainer

A maintainer is removed from [`MAINTAINERS`](MAINTAINERS) in any of these cases:

- **By their own request.** A maintainer may step down at any time.
- **By inactivity.** After a sustained period with no reviews, merges, or
  participation, the remaining maintainers may remove the inactive maintainer by
  the same consensus process used to add one. An emeritus note acknowledging
  their contributions is encouraged.
- **By consensus.** In the rare case of conduct incompatible with the
  [Code of Conduct](https://sumatoshi-tech.github.io/codefang/contributing/code-of-conduct/),
  the remaining maintainers may remove a maintainer by consensus.

Removal is recorded by a pull request that updates [`MAINTAINERS`](MAINTAINERS)
and revokes repository write access.

## Changing this document

This document is itself governed by the decision process above: propose changes
through a pull request, and the maintainers decide. Keep it honest — it must
always describe how the project actually operates, not how we wish it did.

## See also

- [`MAINTAINERS`](MAINTAINERS) — the authoritative maintainer list.
- [`CONTRIBUTING.md`](CONTRIBUTING.md) — how to file issues, propose changes, and
  the versioning policy.
- [Code of Conduct](https://sumatoshi-tech.github.io/codefang/contributing/code-of-conduct/) — the behavior expected of everyone who participates.

<!-- Template: MADR 4.0 (https://adr.github.io/madr/). -->

# 0001 — Record architecture decisions

- Status: accepted
- Date: record-architecture-decisions
- Deciders: @dmytrogajewski

## Context and problem statement

Codefang is a reincarnation of the abandoned `hercules` tool, rebuilt as a Rust workspace. The rewrite made several significant choices — a two-binary split, a Tree-sitter-backed Universal Abstract Syntax Tree, libgit2 via the Rust `git2` crate, a streaming pipeline, MkDocs Material for docs, and the Apache-2.0 license. These decisions currently live only as prose scattered across the README and `site/architecture/`, or as feature-scoped FRDs under `specs/frds/`. New contributors cannot see *why* a choice was made, what was considered, or whether a past decision still holds. How should the project capture architecturally significant decisions so the reasoning survives the people who made it?

## Decision drivers

- New contributors need the rationale behind a decision, not just its result.
- Decisions must be reviewable in the same pull-request flow as code.
- The record must outlive any individual maintainer.
- FRDs under `specs/frds/` are feature-scoped; they do not capture cross-cutting architectural rationale.
- The format must be lightweight enough that writing an ADR is not a deterrent.

## Considered options

- Architecture Decision Records (ADRs) in MADR 4.0 shape, stored in the repository.
- Keep recording decisions only in the README and `site/architecture/` prose.
- Reuse the existing `specs/frds/` FRD format for architectural decisions.
- A wiki or external document store outside version control.

## Decision outcome

Chosen option: "ADRs in MADR 4.0 shape, stored in the repository", because they version with the code, review through the normal pull-request flow, and carry the Context, Drivers, Options, and Consequences that prose and feature-scoped FRDs omit.

ADRs live as numbered Markdown files at `docs/adr/NNNN-<slug>.md`. The canonical files are surfaced in the MkDocs site (under `docs_dir: site`) through a `site/adr` symlink so the documentation site can render them without duplicating content. Each ADR is immutable once accepted; a later decision that overturns it is a new ADR that marks the old one superseded and links to it.

### Consequences

- Good: The reasoning behind each significant choice is discoverable, reviewable, and durable.
- Good: A superseded decision and the decision that replaced it are linked, so the history of the architecture reads as a chain.
- Neutral: Every architecturally significant change now carries the small overhead of authoring an ADR.
- Bad: An ADR can drift from the implementation if a decision changes without a follow-up ADR; the governance process must require an ADR for significant changes to mitigate this.

## Pros and cons of the options

### ADRs in MADR 4.0 shape

- Good: Captures context, drivers, considered options, and consequences in a consistent shape.
- Good: Versions with the code and reviews in the normal pull-request flow.
- Neutral: Requires a numbering and supersession convention, established by this ADR.

### Prose in README and site/architecture/

- Good: No new convention to learn.
- Bad: Records the *what* but rarely the *why*, the alternatives, or the trade-offs.
- Bad: Prose is rewritten in place, so the history of a decision is lost.

### Reuse specs/FRDs/ FRDs

- Good: An existing, familiar format.
- Bad: FRDs are feature-scoped and describe what to build, not cross-cutting architectural rationale.

### External wiki or document store

- Good: Rich editing surface.
- Bad: Drifts out of sync with the code, is not reviewed in pull requests, and may not outlive the hosting service.

## Links

- Supersedes: none
- Superseded by: none

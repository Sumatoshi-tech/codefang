# Third-party notices

Codefang is licensed under [Apache-2.0](LICENSE). It reuses or bundles content
from the projects below. This file names each borrowed component and points to
its upstream licence; it does not reproduce the full licence texts. Consult the
linked upstream sources for the authoritative terms.

The full dependency graph (the MIT / Apache-2.0 / BSD Go modules pulled in via
`go.mod`) is surfaced by the release tooling and is not enumerated here. The
entries below cover content that is borrowed verbatim, vendored, or otherwise
warrants an explicit notice.

## Borrowed content

| Component | What it is | Upstream licence | Link |
| --- | --- | --- | --- |
| Contributor Covenant 2.1 | Text adapted into our [code of conduct](site/contributing/code-of-conduct.md) | CC-BY-4.0 | <https://www.contributor-covenant.org/version/2/1/code_of_conduct.html> |
| Tree-sitter grammars | Per-language parser grammars consumed via `go-sitter-forest` (one module per language in `go.mod`) | Per-grammar (mostly MIT / Apache-2.0) | <https://github.com/alexaandru/go-sitter-forest> |
| enry / Linguist dataset | Language-identification data and heuristics used for file classification | MIT | <https://github.com/src-d/enry> |
| libgit2 | Git plumbing library vendored as a submodule under `third_party/libgit2/` and used via `git2go` | GPL-2.0 with a linking exception (see `third_party/libgit2/COPYING`) | <https://github.com/libgit2/libgit2> |

## Notes

- The Tree-sitter grammars are a category: each language grammar listed in
  `go.mod` under `github.com/alexaandru/go-sitter-forest/*` carries its own
  upstream licence. See the linked aggregator for the per-grammar provenance.
- libgit2 ships under GPL-2.0 **with a linking exception** that permits linking
  the compiled library into other programs and distributing those combinations
  without restriction. The exception and full text live in
  `third_party/libgit2/COPYING`.

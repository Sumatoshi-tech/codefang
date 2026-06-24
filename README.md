# Codefang

<p align="center">
  <img src="assets/uast.png" alt="Codefang Logo" width="250">
</p>

[![CI](https://github.com/Sumatoshi-tech/codefang/actions/workflows/ci.yml/badge.svg)](https://github.com/Sumatoshi-tech/codefang/actions/workflows/ci.yml)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

**The heavy lifter for your codebase — deep code analysis through structure and history.**

Codefang understands your code as structure (an abstract syntax tree) and as
history (Git), not just as text. It ships two binaries: `uast` parses source
into a Universal Abstract Syntax Tree across 60+ languages, and `codefang` runs
static and history analyzers over that structure and over Git repositories.

For deep dives — architecture, per-analyzer references, the metric models — see
the [documentation site](https://sumatoshi-tech.github.io/codefang/) (built from
`site/` with MkDocs Material).

---

## What it does

- **UAST parsing** — `uast` turns source code into one normalized tree shape,
  so a single analyzer works across Go, Python, JavaScript, Rust, Java, C++ and
  more (Tree-sitter under the hood).
- **Static analysis** — complexity, cohesion, comments, clones, composition,
  Halstead, and imports, computed from the UAST.
- **History analysis** — burndown, developers, couples, file-history, imports,
  quality, sentiment, shotness, typos, and anomaly, computed by walking Git
  history through libgit2.
- **Stable machine output** — `--format json` (and `yaml`, `ndjson`,
  `timeseries`, `bin`, `text`, `compact`, `plot`). The JSON byte stream is a
  frozen contract, verified by the differential parity gate (see below).

## Why

The original `hercules` mined Git history at scale but understood code only as
diffs. Codefang keeps the history mining and adds a structural layer (the UAST),
so one analyzer reasons about meaning across many languages. The project is now
a Rust workspace; the original Go implementation has been removed.

---

## Install / build

Codefang is a single Rust workspace rooted at the repository root. You need a
recent stable Rust toolchain and a C toolchain (a C/C++ compiler and CMake) —
the `git2` crate builds a vendored copy of **libgit2** from
`third_party/libgit2`, so no system libgit2 is required.

The quickest path — clone, then one `make install` that builds both binaries
and puts them on your `PATH` (`~/.cargo/bin`):

```bash
git clone --recurse-submodules https://github.com/Sumatoshi-tech/codefang.git
cd codefang
make install
```

`make install` also initializes the submodule, so if you cloned without
`--recurse-submodules` it still works. Run `make help` to see the other targets
(`build`, `test`, `clean`).

Prefer plain cargo, or just want the binaries in `target/release/` without
touching `PATH`? Use `make build` (equivalently
`cargo build --release -p codefang -p uast`) and invoke them by full path.

Check the install:

```console
$ codefang version
```

```console
$ uast version
```

---

## Usage

`uast` and `codefang` are separate tools (see [ADR-0002](docs/adr/0002-two-binary-split-uast-codefang.md)).

Parse a source file into a UAST:

```console
$ uast parse main.rs
```

Run a static analyzer over a checkout (UAST is built internally per file):

```console
$ codefang run --analyzers static/complexity --format json --head /path/to/repo
```

Run a history analyzer by walking Git history (use `--limit` to cap commits):

```console
$ codefang run --analyzers history/burndown --format json --limit 50 /path/to/repo
```

List every analyzer ID:

```console
$ codefang run --list-analyzers
```

Analyzer IDs are `static/<name>` and `history/<name>`; `-a` accepts globs
(`static/*`, `history/*`, `*`).

---

## Differential parity gate

Machine-report output is a frozen contract. The original Go binaries are kept
solely as a **frozen oracle** at `build/bin/{codefang,uast}` — the Go source is
gone, only the compiled reference binaries remain. The parity harness under
`tests/compat/` diffs the Rust binary's bytes against that oracle:

```bash
cd tests/compat
python3 oracle/oracle.py --n-go 3 --quiet -- \
  run --checkpoint=false --resume=false --no-cache --workers 1 \
  --analyzers history/burndown --format json --limit 50 /path/to/repo
```

The oracle runs the Go reference N≥3 times, classifies each output field as
stable or run-to-run variant, requires the Rust output to be byte-identical on
stable fields, and rejects any attempt to blank a stable field. Exit code `0`
means parity holds. See `tests/compat/README.md` for the full system.

The CLI examples in this README and in `docs/` are themselves executed against
the built Rust binary by the `doc-examples` test crate
(`cargo test -p doc-examples`), so the commands shown here are runnable, not
illustrative.

---

## Documentation

- [Architecture overview](site/architecture/overview.md) and the
  [UAST system](site/architecture/uast.md).
- Per-analyzer references under [`site/analyzers/`](site/analyzers/) and the
  metric models under [`site/explanation/`](site/explanation/).
- Architecture Decision Records under [`docs/adr/`](docs/adr/).
- Per-crate Rust API docs: `cargo doc --open`.

---

## Contributing

PRs are welcome. Read the [Contributing guide](CONTRIBUTING.md) for the build,
the test gates, and commit conventions. Architecturally significant changes are
recorded as ADRs (see [ADR-0001](docs/adr/0001-record-architecture-decisions.md)).

## Security

Report vulnerabilities privately through GitHub's "Report a vulnerability"
button on the Security tab. See the [Security policy](SECURITY.md).

## License

Codefang is released under the [Apache-2.0](LICENSE) license.

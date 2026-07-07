# Installation

This guide covers how to build Codefang and its companion UAST parser from the
Rust workspace.

---

## Prerequisites

Before you begin, make sure the following tools are available on your system:

| Requirement | Minimum Version | Notes |
|-------------|-----------------|-------|
| **Rust** | stable (recent) | The project is a Cargo workspace under `rust/` |
| **Git** | 2.x | Used to clone (with submodules) and by history analyzers |
| **C compiler** | GCC or Clang | Builds the vendored libgit2 via the `git2` crate |
| **CMake** | 3.14+ | Needed by the `git2` crate's build of vendored libgit2 |

!!! info "Vendored libgit2"
    Codefang links against **libgit2** for high-performance Git repository
    access. The `git2` crate's `vendored-libgit2` feature compiles the pinned
    sources in `third_party/libgit2` at build time, so no system libgit2 is
    required — but a C toolchain and CMake are. See
    [ADR-0003](../adr/0003-libgit2-via-cgo.md) for the rationale.

---

## Build from source

Clone with submodules (the vendored libgit2 lives in a submodule), then build
both binaries with Cargo:

```bash
git clone --recurse-submodules https://github.com/Sumatoshi-tech/codefang.git
cd codefang/rust
cargo build --release -p codefang -p uast
```

The binaries are produced at `target/release/codefang` and
`target/release/uast`. Add that directory to your `PATH`:

```bash
export PATH="$PWD/target/release:$PATH"
```

!!! tip "Add to your shell profile"
    Append an `export` line pointing at the absolute path of
    `target/release` to your `~/.bashrc`, `~/.zshrc`, or equivalent so the
    binaries are always available.

---

## Docker

A multi-stage `Dockerfile` is included at the repository root:

```bash
docker build -t codefang .
```

Run analysis on a local repository:

```bash
docker run --rm -v "$(pwd):/repo" codefang run -a static/complexity --head /repo
```

Run history analysis (the container needs the full `.git` directory):

```bash
docker run --rm -v "$(pwd):/repo" codefang run -a history/burndown --format yaml --limit 50 /repo
```

!!! note "Image size"
    The production image is built on a minimal base. The vendored libgit2 and
    Tree-sitter grammars are compiled into the binary at build time, so no
    additional runtime dependencies are needed inside the container.

---

## Verify installation

After building, confirm both binaries work:

=== "codefang"

    ```console
    $ codefang version
    ```

=== "uast"

    ```console
    $ uast version
    ```

---

## Platform notes

### Linux

Fully supported. Install build essentials if you do not already have a C
compiler and CMake:

=== "Debian / Ubuntu"

    ```bash
    sudo apt-get install build-essential cmake pkg-config
    ```

=== "Fedora / RHEL"

    ```bash
    sudo dnf install gcc gcc-c++ cmake pkg-config
    ```

=== "Arch Linux"

    ```bash
    sudo pacman -S base-devel cmake pkg-config
    ```

### macOS

Fully supported. Xcode Command Line Tools provide the required C toolchain:

```bash
xcode-select --install
brew install cmake pkg-config
```

### Windows

Not officially supported at this time. You can use **WSL 2** with a Linux
distribution and follow the Linux instructions above.

---

## Next steps

With both binaries built, head to the [Quick Start](quickstart.md) guide to run
your first analysis.

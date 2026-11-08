# Verification

- Workspace suite: 3230 tests passed, 1 ignored, zero failures.
- Command: `TMPDIR="$PWD/.tmp/shotness-doc-tests" make -o submodules test`.
- Includes unit, integration, 64 language-matrix tests, Markdown examples and doctests.
- `make -o submodules lint`: exit 0; Clippy across workspace/all-targets with warnings denied.
- `make -o submodules deadcode`: exit 0; dead code denied across workspace/all-targets.
- `cargo fmt --all -- --check`: exit 0.
- `make -o submodules build`: exit 0.
- `make -o submodules install`: exit 0; both installed binaries replaced.
- Installed mixed-repository JSON/text reports agree: 71 entities and 141 touches.
- Every name and per-entity count matches the 63 fixture-file expectations.
- Previous binary: 37 entities and 72 touches on the same repository.

The first workspace run passed runtime tests but six doctest links crashed with
SIGBUS under `/tmp`. Those six doctests passed with disk-backed temporary files;
the entire workspace rerun with the same temporary-file setting also passed.
No checks were disabled. The root cause of the linker crash is not established.

`-o submodules` uses the existing submodule checkout without invoking Git.
Raw logs: `/tmp/codefang-shotness-languages/`.
Rollback binaries: `/tmp/codefang-shotness-languages/before-bin/`.

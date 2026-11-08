# Verification

- `make -o submodules test`: exit 0; 3162 tests passed, 1 ignored.
- Includes unit, integration, CLI end-to-end, Markdown examples and Rust doctests.
- `make -o submodules lint`: exit 0; warnings denied across all workspace targets.
- `make -o submodules deadcode`: exit 0; dead code denied across all workspace targets.
- `cargo fmt --all -- --check`: exit 0.
- `make -o submodules build`: exit 0.
- `make -o submodules install`: exit 0; both executables replaced.
- Installed `codefang` with the screenshot's exact flags: 14 nodes, 21 changes, 29 couplings.
- Installed JSON and text totals agree.
- Installed `uast`: nested classes have names `Outer`, `Companion`, `Inner`, `Factory`.

The direct Shotness CLI test suite passes all 18 tests. Eight new regressions
cover the exact flags, class/object/companion qualification, script changes
and couplings, annotated generic extension names, and nested companion names.

`-o submodules` uses the existing submodule checkout without invoking Git.
Go-specific commands do not apply to this Rust workspace.
Raw logs are under `/tmp/codefang-kotlin-repro/shotness-*.log`.

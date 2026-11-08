# Verification

- `make -o submodules test`: exit 0; 3154 tests passed, 1 ignored.
- Includes unit, integration, CLI end-to-end, Markdown examples and Rust doctests.
- `make -o submodules lint`: exit 0; Clippy workspace/all-targets with warnings denied.
- `make -o submodules deadcode`: exit 0; workspace/all-targets check with dead code denied.
- `cargo fmt --all -- --check`: exit 0.
- `make -o submodules build`: exit 0.
- `make -o submodules install`: exit 0; both executables replaced.
- Installed `codefang`: public Kotlin project reports 96 functions (previously 0).
- Installed `uast`: companion-object Kotlin fixture parses and yields one function.

`-o submodules` uses the existing submodule checkout without invoking Git.
Go-specific vet/race/coverage commands do not apply to this Rust workspace.
No production code changed; the fix replaces installed builds lacking Kotlin support.

Raw command logs: `/tmp/codefang-kotlin-repro/{test,lint,deadcode,fmt,build,install}.log`.

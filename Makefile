# Codefang — convenience wrapper around cargo for the common workflows.
#
# The project is a single Rust workspace rooted here (Cargo.toml). The two
# binaries are `codefang` (bins/codefang) and `uast` (bins/uast). The git2 crate
# builds a vendored libgit2 from the third_party/libgit2 submodule, so a C
# toolchain + CMake are required and the submodule must be present.

CARGO ?= cargo

.DEFAULT_GOAL := build

.PHONY: help
help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| sort \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

.PHONY: submodules
submodules: ## Ensure the libgit2 submodule is checked out
	@git submodule update --init --recursive

.PHONY: build
build: submodules ## Build both binaries in release mode (target/release/)
	$(CARGO) build --release -p codefang -p uast

.PHONY: install
install: submodules ## Build and install codefang + uast onto PATH (~/.cargo/bin)
	$(CARGO) install --path bins/codefang --locked
	$(CARGO) install --path bins/uast --locked
	@echo
	@echo "Installed codefang and uast to $${CARGO_HOME:-$$HOME/.cargo}/bin"
	@echo "Make sure that directory is on your PATH, then run: codefang version"

.PHONY: test
test: submodules ## Run the workspace test suite
	$(CARGO) test --workspace

.PHONY: lint
lint: submodules ## Clippy across the workspace; every warning is an error
	$(CARGO) clippy --workspace --all-targets -- -D warnings

.PHONY: deadcode
# rustc's `dead_code` lint reports unreachable PRIVATE items (a `pub` item in a
# library crate always looks reachable to it, so a clean run means "no private
# code is unreachable", not "no exported function is unused"). A pub-API
# reachability audit needs external tooling (cargo-deadstats-class) which is
# deliberately not vendored here.
deadcode: submodules ## Fail the build on any `dead_code` finding
	RUSTFLAGS="-D dead_code" $(CARGO) check --workspace --all-targets

.PHONY: clean
clean: ## Remove build artifacts
	$(CARGO) clean

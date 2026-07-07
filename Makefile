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

.PHONY: clean
clean: ## Remove build artifacts
	$(CARGO) clean

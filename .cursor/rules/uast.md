# UAST Implementation Rules

- Always use `Parser` (`parser.go`) as the entrypoint for UAST parsing.
- Provider loading must use `loader.go` and mapping files; do not hardcode providers.
- Provider instantiation (native, treesitter, external) must go through `factory.go`.
- `TreeSitterProvider` logic should remain isolated and only be updated for interface changes.
- All new languages must have a mapping file and test coverage.
- All code must be clean, idiomatic Go, with top-down function ordering, minimal comments, and descriptive names.
- All changes must be accompanied by tests; run `make test` after every change.
- Do not reintroduce legacy `EmbeddedProvider` or `Factory` abstractions.
- Use table-driven tests for all new features.
- Keep modules under 500 lines; split if necessary.
- Prefer non-recursive implementations for conversion code.
- In server mode, logs go to stdout as slog JSON; in CLI mode, logs go to a file.
- Use `spf13/viper` for config and `gofr` for web server if needed.
- Use distroless base images for Docker.
- Use latest Tree-sitter version (0.25) for UAST parsing.
- Do not use Babelfish when `--uast-provider=embedded` is set.
- Always run `make clean && make` before `make test`.
- Remove all debug logs from stdout unless debug flag is set.
- All code must be self-explanatory; avoid comments except for public API and complex logic. 
- Follow clean code principles
- Each if statement block should include only one function called inside, e.g.:

    if expr {
    funCall(a)
    }

    3. each expression in if should be a function, e.g.

    Instead:

    if a < b || b > 3 {
    ...
    }

    Do:

    if isSomething(a,b) {
    anotherFn(s)
    }

- Order function from caller to callee
- Use clear understandable function names that are not overcomplicated
- Prefer to extend modules test suite, do not write ad-hoc tests
- Do not write code comments, unless its GoDoc
- Write GoDoc with examples for each exported function
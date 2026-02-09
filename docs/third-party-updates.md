# Third-Party Update Strategy

This project uses two dependency tracks for parser/history performance:

- `libgit2`: source snapshot in `third_party/libgit2` (built locally via `make libgit2`)
- tree-sitter bindings: Go module dependencies (`go.mod`)

## Strategy Decision (Roadmap 2.1)

- Keep tree-sitter in Go modules instead of full source vendoring.
- Keep libgit2 as local third-party source in `third_party/libgit2`.
- Use deterministic refs (tags or SHAs) for both tracks.
- Avoid git-based update flows in the automation path.

## Update Commands

Update only libgit2:

```bash
make deps-update-libgit2 LIBGIT2_REF=v1.9.1
```

Update only tree-sitter related modules:

```bash
make deps-update-treesitter \
  TREE_SITTER_BARE_REF=v1.11.0 \
  SITTER_FOREST_REF=v1.9.163 \
  SITTER_FOREST_GO_REF=v1.9.4
```

Update both in one run:

```bash
make deps-update-all \
  LIBGIT2_REF=v1.9.1 \
  TREE_SITTER_BARE_REF=v1.11.0 \
  SITTER_FOREST_REF=v1.9.163 \
  SITTER_FOREST_GO_REF=v1.9.4
```

## Dry Run

Use the script directly to preview changes:

```bash
./scripts/update-third-party.sh --dry-run --mode all \
  --libgit2-ref v1.9.1 \
  --tree-sitter-bare-ref v1.11.0 \
  --sitter-forest-ref v1.9.163 \
  --sitter-forest-go-ref v1.9.4
```

## Post-Update Validation

```bash
make libgit2
go test ./pkg/uast/... -count=1
go test -race -p=1 ./pkg/uast/... -count=1
make lint
```

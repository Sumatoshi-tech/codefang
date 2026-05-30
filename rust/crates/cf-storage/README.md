# cf-storage

Rust port of the Go package `internal/storage`.

Storage backend abstraction. Today its single responsibility is **atomic file
writes** (`WriteAtomic`): write a payload to a target path such that a concurrent
reader, or a crash mid-write, never observes a truncated or partially written
file.

Used by `cf-cache`, `cf-analyze`, and the `render` command (which writes
`report.json` plus HTML artifacts; see DESIGN §4.1, report files are mode `0640`).

## API

This is a faithful port of Go's
`WriteAtomic(path string, perm os.FileMode, write func(w io.Writer) error) error`
— it takes a **writer callback**, not a finished byte slice, so the caller can
stream serialization directly into the file:

```rust
use std::io::Write;

cf_storage::write_atomic("out/report.json", 0o640, |w| {
    w.write_all(&serialized_bytes)
})?;
```

On error, `write_atomic` returns an `AtomicWriteError` whose `Display` is
byte-identical to Go's wrapped message (e.g. `atomic create /x/y.tmp: ...`,
`atomic write /x/y: ...`).

## Atomicity (matches `atomicfile.go` step-for-step)

1. Open `<path>.tmp` with `O_WRONLY | O_CREATE | O_TRUNC` and the given `perm`
   (a **fixed** `.tmp` sibling, not a random name — same as Go).
2. Call the `write` closure with the open file.
3. `fsync` (`fd.Sync()`) for durability before the rename.
4. Close the file.
5. `rename(<path>.tmp, <path>)` — atomic on POSIX.

If `write`, `sync`, `close`, or `rename` fails, the `.tmp` file is removed and a
wrapped error is returned (best-effort removal, matching Go's unchecked
`os.Remove`).

## Serialization boundary

This crate emits **no** machine-format report bytes itself — the caller's `write`
closure produces them. It therefore does not depend on `cf-gojson` / `cf-goyaml`.
Callers serialize through those crates (DESIGN §2) and write the finished bytes to
the provided writer.

## Dependencies

None at runtime (uses only `std`). `tempfile` is a dev-dependency for tests.

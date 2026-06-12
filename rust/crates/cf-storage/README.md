# cf-storage

Storage backend abstraction. Its single responsibility is **atomic file
writes** (`write_atomic`): write a payload to a target path such that a
concurrent reader, or a crash mid-write, never observes a truncated or
partially written file.

Used by `cf-cache`, `cf-analyze`, and the `render` command (which writes
`report.json` plus HTML artifacts; report files are mode `0640`).

## API

`write_atomic(path, perm, write)` takes a **writer callback**, not a finished
byte slice, so the caller can stream serialization directly into the file:

```rust
use std::io::Write;

cf_storage::write_atomic("out/report.json", 0o640, |w| {
    w.write_all(&serialized_bytes)
})?;
```

On error, `write_atomic` returns an `AtomicWriteError` whose `Display` wording
is part of the CLI compatibility contract (e.g. `atomic create /x/y.tmp: ...`,
`atomic write /x/y: ...`), pinned by the differential gate in
`rust/tests/compat`.

## Atomicity

1. Open `<path>.tmp` with `O_WRONLY | O_CREATE | O_TRUNC` and the given `perm`
   (a **fixed** `.tmp` sibling, not a random name).
2. Call the `write` closure with the open file.
3. `fsync` for durability before the rename.
4. Close the file.
5. `rename(<path>.tmp, <path>)` — atomic on POSIX.

If `write`, `sync`, `close`, or `rename` fails, the `.tmp` file is removed and
a wrapped error is returned (best-effort removal; its result is ignored).

## Serialization boundary

This crate emits **no** machine-format report bytes itself — the caller's
`write` closure produces them. It therefore does not depend on `cf-gojson` /
`cf-goyaml`. Callers serialize through those crates and write the finished
bytes to the provided writer.

## Dependencies

None at runtime (uses only `std`). `tempfile` is a dev-dependency for tests.

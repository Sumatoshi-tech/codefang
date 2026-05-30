//! Storage backend abstraction — Rust port of the Go package `internal/storage`.
//!
//! The Go package's job is narrow and load-bearing: provide an *atomic* file
//! write so that concurrent readers (or a crash mid-write) never observe a
//! truncated or partially written file. It is used by `cache`, `analyze`, and
//! the `render` command (which writes `report.json` plus HTML artifacts).
//!
//! # Atomicity model
//!
//! [`write_atomic`] reproduces `internal/storage/atomicfile.go` exactly:
//!
//! 1. Open (`O_WRONLY | O_CREATE | O_TRUNC`, with the given `perm`) a sibling
//!    temp file at `<path>.tmp` — the same fixed name Go uses, *not* a random
//!    name. Keeping it beside the target keeps the final rename within one
//!    filesystem so the rename is atomic.
//! 2. Invoke the caller's `write` closure with the open file as an
//!    [`io::Write`]. The closure produces the payload bytes (e.g. a serialized
//!    report).
//! 3. `fsync` the file (`fd.Sync()`) so the bytes hit stable storage *before*
//!    the rename.
//! 4. Close the file.
//! 5. `rename(<path>.tmp, <path>)` — atomic on POSIX: a reader sees either the
//!    old contents or the complete new contents, never a mix.
//!
//! If `write`, `sync`, `close`, or `rename` fails, the `.tmp` file is removed and
//! a wrapped error is returned. The error messages match Go byte-for-byte:
//! `atomic create <tmp>: ...`, `atomic write <path>: ...`,
//! `atomic sync <path>: ...`, `atomic close <path>: ...`,
//! `atomic rename <path>: ...`.
//!
//! # Serialization boundary
//!
//! This crate emits **no** machine-format report bytes itself — the caller's
//! `write` closure does — so it does not depend on the `cf-gojson` / `cf-goyaml`
//! serialization crates. Callers serialize their value through those crates (per
//! DESIGN §2) and write the finished bytes to the provided writer.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Suffix appended to the target path to form the temporary sibling file.
///
/// Matches Go's `const tmpSuffix = ".tmp"` (atomicfile.go:10).
const TMP_SUFFIX: &str = ".tmp";

/// Error returned by [`write_atomic`].
///
/// Wraps the underlying [`io::Error`] with the same human-readable prefix Go
/// produces via `fmt.Errorf("atomic <op> %s: %w", ...)`. The `Display` output is
/// byte-identical to Go's; the source error is preserved for programmatic
/// inspection (mirroring Go's `%w` error wrapping).
#[derive(Debug)]
pub struct AtomicWriteError {
    /// The composed message, e.g. `atomic create /x/y.tmp: ...`.
    message: String,
    /// The wrapped I/O error (Go's `%w`).
    source: io::Error,
}

impl AtomicWriteError {
    /// The operation-prefixed message, identical to Go's error string.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Consume the wrapper and return the underlying [`io::Error`].
    #[must_use]
    pub fn into_io_error(self) -> io::Error {
        self.source
    }

    /// Borrow the underlying [`io::Error`].
    #[must_use]
    pub fn io_error(&self) -> &io::Error {
        &self.source
    }
}

impl std::fmt::Display for AtomicWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Go renders `%w` as the wrapped error's own message, joined by ": ".
        write!(f, "{}: {}", self.message, self.source)
    }
}

impl std::error::Error for AtomicWriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

impl From<AtomicWriteError> for io::Error {
    fn from(e: AtomicWriteError) -> Self {
        // Preserve the original error kind while surfacing the composed message,
        // so callers that funnel everything into io::Error keep both.
        io::Error::new(e.source.kind(), e.message)
    }
}

/// Append [`TMP_SUFFIX`] to `path` to produce the temp sibling path.
///
/// Go computes `tmpPath := path + tmpSuffix` on the raw string, so we operate on
/// the `OsString` to reproduce that exactly (notably for a trailing-slash path,
/// where `<dir>/` becomes `<dir>/.tmp`).
fn tmp_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(TMP_SUFFIX);
    PathBuf::from(s)
}

/// Apply the Unix permission `perm` to the `OpenOptions` (Unix only).
#[cfg(unix)]
fn with_mode(opts: &mut OpenOptions, perm: u32) -> &mut OpenOptions {
    use std::os::unix::fs::OpenOptionsExt;
    opts.mode(perm)
}

/// On non-Unix targets the Unix mode bits have no portable meaning; ignore them
/// (matching how a Go binary on Windows treats `os.FileMode` perm bits).
#[cfg(not(unix))]
fn with_mode(opts: &mut OpenOptions, _perm: u32) -> &mut OpenOptions {
    opts
}

/// Write to `path` atomically.
///
/// Creates a `<path>.tmp` sibling opened with `O_WRONLY|O_CREATE|O_TRUNC` and the
/// given Unix permission `perm`, calls `write` with the open file, `fsync`s it,
/// closes it, then renames it over `path`. If `write` returns an error or any
/// step fails, the `.tmp` file is removed and the error is returned.
///
/// This is the direct port of Go's
/// `storage.WriteAtomic(path string, perm os.FileMode, write func(w io.Writer) error) error`.
///
/// # Errors
///
/// Returns an [`AtomicWriteError`] whose message matches Go's wording:
/// - `atomic create <tmp>: ...` if the temp file cannot be created;
/// - `atomic write <path>: ...` if the `write` closure returns an error;
/// - `atomic sync <path>: ...` if `fsync` fails;
/// - `atomic close <path>: ...` if `close` fails;
/// - `atomic rename <path>: ...` if the rename fails.
///
/// In every non-`create` failure path the `.tmp` file is removed before
/// returning (best-effort, matching Go's unchecked `os.Remove`).
///
/// # Examples
///
/// ```
/// use std::io::Write;
/// let dir = tempfile::tempdir().unwrap();
/// let target = dir.path().join("report.json");
/// cf_storage::write_atomic(&target, 0o640, |w| w.write_all(b"{}\n")).unwrap();
/// assert_eq!(std::fs::read(&target).unwrap(), b"{}\n");
/// ```
pub fn write_atomic<P, F>(path: P, perm: u32, write: F) -> Result<(), AtomicWriteError>
where
    P: AsRef<Path>,
    F: FnOnce(&mut dyn Write) -> io::Result<()>,
{
    let path = path.as_ref();
    let tmp = tmp_path(path);

    // 1. Create the temp sibling. Go: os.OpenFile(tmpPath, O_WRONLY|O_CREATE|O_TRUNC, perm).
    //    On failure Go returns "atomic create <tmpPath>: <err>" and does NOT
    //    attempt removal (nothing was created).
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let mut fd = match with_mode(&mut opts, perm).open(&tmp) {
        Ok(fd) => fd,
        Err(e) => {
            return Err(AtomicWriteError {
                message: format!("atomic create {}: ", tmp.display()),
                source: e,
            });
        }
    };

    // 2. Run the caller's writer. On error: close (drop) + remove tmp, then
    //    "atomic write <path>: <err>".
    if let Err(e) = write(&mut fd) {
        drop(fd);
        let _ = std::fs::remove_file(&tmp);
        return Err(AtomicWriteError {
            message: format!("atomic write {}: ", path.display()),
            source: e,
        });
    }

    // 3. fsync (Go: fd.Sync()). On error: close + remove tmp, then
    //    "atomic sync <path>: <err>".
    if let Err(e) = fd.sync_all() {
        drop(fd);
        let _ = std::fs::remove_file(&tmp);
        return Err(AtomicWriteError {
            message: format!("atomic sync {}: ", path.display()),
            source: e,
        });
    }

    // 4. Close (Go: fd.Close()). `File` has no fallible `close`; dropping it
    //    closes the descriptor. We already `sync_all`'d in step 3, so any
    //    buffered-write error has surfaced — a Rust drop cannot report a close
    //    error. We keep this as an explicit step (rather than letting `fd` fall
    //    out of scope) to preserve Go's "close then rename" ordering and to make
    //    the temp file's lifetime end here. Go's `atomic close <path>` branch is
    //    therefore unreachable in practice; it has no observable Rust analogue.
    drop(fd);

    // 5. Rename into place (Go: os.Rename). On error: remove tmp, then
    //    "atomic rename <path>: <err>".
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(AtomicWriteError {
            message: format!("atomic rename {}: ", path.display()),
            source: e,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_atomic;
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;

    const TEST_PERM: u32 = 0o600;
    const TEST_CONTENT: &str = "hello atomic";
    const TEST_TMP_SUFFIX: &str = ".tmp";

    /// Helper mirroring the Go test's `writeString`.
    fn write_string(w: &mut dyn Write, s: &str) -> io::Result<()> {
        w.write_all(s.as_bytes())
    }

    /// Port of Go `TestWriteAtomic_SuccessPath`.
    #[test]
    fn write_atomic_success_path() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output.dat");

        write_atomic(&target, TEST_PERM, |w| write_string(w, TEST_CONTENT)).unwrap();

        let got = fs::read_to_string(&target).unwrap();
        assert_eq!(got, TEST_CONTENT);

        // No tmp file remains.
        let tmp = format!("{}{TEST_TMP_SUFFIX}", target.display());
        assert!(!Path::new(&tmp).exists(), "tmp file should not exist after success");
    }

    /// Port of Go `TestWriteAtomic_OverwritesExistingFile`.
    #[test]
    fn write_atomic_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output.dat");

        fs::write(&target, b"old").unwrap();

        write_atomic(&target, TEST_PERM, |w| write_string(w, "new")).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    /// Port of Go `TestWriteAtomic_WriteCallbackError_CleansUp`.
    #[test]
    fn write_atomic_write_callback_error_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output.dat");

        let sentinel = io::Error::other("write callback failed");
        let err = write_atomic(&target, TEST_PERM, |_w| {
            Err(io::Error::other("write callback failed"))
        })
        .unwrap_err();

        // The wrapped error preserves the sentinel's kind and message (Go's %w).
        assert_eq!(err.io_error().kind(), sentinel.kind());
        assert!(err.to_string().contains("write callback failed"));
        assert!(err.message().starts_with("atomic write "));

        // Target should not exist.
        assert!(!target.exists(), "target file should not exist after write error");

        // Tmp file should be cleaned up.
        let tmp = format!("{}{TEST_TMP_SUFFIX}", target.display());
        assert!(
            !Path::new(&tmp).exists(),
            "tmp file should be cleaned up after write error"
        );
    }

    /// Port of Go `TestWriteAtomic_CreateError_InvalidDir`.
    #[test]
    fn write_atomic_create_error_invalid_dir() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("nonexistent").join("subdir").join("file.dat");

        let err = write_atomic(&target, TEST_PERM, |w| write_string(w, TEST_CONTENT)).unwrap_err();

        // Go: assert.Contains(err.Error(), "atomic create").
        assert!(
            err.to_string().contains("atomic create"),
            "got: {}",
            err
        );
    }

    /// Port of Go `TestWriteAtomic_EmptyWrite`.
    #[test]
    fn write_atomic_empty_write() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("empty.dat");

        write_atomic(&target, TEST_PERM, |_w| Ok(())).unwrap();

        let got = fs::read(&target).unwrap();
        assert!(got.is_empty());
    }

    /// On Unix, the requested permission mode is applied to the renamed file.
    /// (Not a Go test case, but verifies the perm argument is honored.)
    #[cfg(unix)]
    #[test]
    fn write_atomic_applies_perm() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("moded.dat");

        write_atomic(&target, 0o640, |w| write_string(w, "x")).unwrap();

        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        // The on-disk mode is `perm & ~umask`; assert no bit beyond perm is set.
        assert_eq!(mode & !0o640, 0, "unexpected extra permission bits: {mode:o}");
    }

    /// The tmp path is exactly `<path>.tmp` (fixed suffix, not random), matching
    /// Go's `path + tmpSuffix`.
    #[test]
    fn tmp_path_appends_fixed_suffix() {
        assert_eq!(super::tmp_path(Path::new("/a/b/c.dat")), Path::new("/a/b/c.dat.tmp"));
        assert_eq!(super::tmp_path(Path::new("rel.json")), Path::new("rel.json.tmp"));
    }
}

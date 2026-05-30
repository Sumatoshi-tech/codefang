//! File handles and the file iterator, ported from `pkg/gitlib/changes.go`
//! (the `File` type) and `pkg/gitlib/file.go` (the `FileIter` type).
//!
//! A [`File`] names a blob within a tree and can fetch its contents on demand;
//! [`FileIter`] is a simple in-memory iterator over a materialized file list,
//! implementing the shared [`cf_alg::PullIterator`] contract so it interoperates
//! with `cf-alg`'s `collect_n` just like the Go `gitlib.FileIter` satisfies
//! `alg.Iterator[*File]`.

use cf_alg::{IteratorError, PullIterator};

use crate::error::Result;
use crate::hash::Hash;
use crate::Repository;

/// A file in a tree, with content accessible on demand.
///
/// Mirrors Go's `gitlib.File`. Borrows the owning [`Repository`] so it can look
/// up its blob lazily (Go stores an unexported `repo` pointer).
#[derive(Clone)]
pub struct File<'repo> {
    /// The file path within the tree (Go exported field `Name`).
    pub name: String,
    /// The blob hash (Go exported field `Hash`).
    pub hash: Hash,
    /// The file mode (Go exported field `Mode`); `0` when unknown.
    pub mode: u16,
    repo: &'repo Repository,
}

impl<'repo> File<'repo> {
    /// Builds a file handle.
    pub(crate) fn new(name: String, hash: Hash, mode: u16, repo: &'repo Repository) -> Self {
        File {
            name,
            hash,
            mode,
            repo,
        }
    }

    /// Returns the file contents (Go `File.Contents` / `File.ContentsContext`).
    ///
    /// # Errors
    ///
    /// Propagates blob-lookup failures.
    pub fn contents(&self) -> Result<Vec<u8>> {
        let blob = self.repo.lookup_blob(self.hash)?;
        Ok(blob.contents())
    }

    /// Returns the blob object for this file (Go `File.Blob` / `File.BlobContext`).
    ///
    /// # Errors
    ///
    /// Propagates blob-lookup failures.
    pub fn blob(&self) -> Result<crate::Blob<'repo>> {
        self.repo.lookup_blob(self.hash)
    }
}

/// An iterator over the files in a tree, ported from Go `gitlib.FileIter`.
///
/// Holds a materialized list of files and yields them in order. Implements
/// [`cf_alg::PullIterator`] (Go's `alg.Iterator[*File]`): [`next`](FileIter::next)
/// returns [`IteratorError::Eof`] when exhausted, and [`close`](FileIter::close)
/// is a no-op marking the iterator drained (Go sets `idx = len(files)`).
pub struct FileIter<'repo> {
    files: Vec<File<'repo>>,
    idx: usize,
}

impl<'repo> FileIter<'repo> {
    /// Builds an iterator over `files`.
    #[must_use]
    pub(crate) fn new(files: Vec<File<'repo>>) -> Self {
        FileIter { files, idx: 0 }
    }

    /// Returns the next file, advancing the cursor.
    ///
    /// Convenience inherent method mirroring Go's `FileIter.Next() (*File, error)`
    /// returning `nil, io.EOF` at the end. Returns `None` at exhaustion.
    #[must_use]
    pub fn next_file(&mut self) -> Option<File<'repo>> {
        if self.idx >= self.files.len() {
            return None;
        }
        let f = self.files[self.idx].clone();
        self.idx += 1;
        Some(f)
    }

    /// Calls `cb` for each remaining file (Go `FileIter.ForEach`).
    ///
    /// Stops and returns the first error produced by `cb`.
    ///
    /// # Errors
    ///
    /// Propagates the first error returned by `cb`.
    pub fn for_each<F>(&self, mut cb: F) -> Result<()>
    where
        F: FnMut(&File<'repo>) -> Result<()>,
    {
        for file in &self.files {
            cb(file)?;
        }
        Ok(())
    }

    /// Marks the iterator drained (Go `FileIter.Close`, a no-op setting
    /// `idx = len(files)`).
    pub fn close(&mut self) {
        self.idx = self.files.len();
    }
}

impl<'repo> PullIterator<File<'repo>> for FileIter<'repo> {
    fn next(&mut self) -> std::result::Result<File<'repo>, IteratorError> {
        self.next_file().ok_or(IteratorError::Eof)
    }

    fn close(&mut self) {
        FileIter::close(self);
    }
}

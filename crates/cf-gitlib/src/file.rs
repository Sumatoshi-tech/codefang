//! File handles ([`File`]) and the file iterator ([`FileIter`]).
//!
//! A [`File`] names a blob within a tree and can fetch its contents on demand;
//! [`FileIter`] is a simple in-memory iterator over a materialized file list,
//! implementing the shared [`cf_alg::PullIterator`] contract so it
//! interoperates with `cf-alg`'s `collect_n`.

use cf_alg::{IteratorError, PullIterator};

use crate::error::Result;
use crate::hash::Hash;
use crate::Repository;

/// A file in a tree, with content accessible on demand.
///
/// Borrows the owning [`Repository`] so it can look up its blob lazily.
#[derive(Clone)]
pub struct File<'repo> {
    /// The file path within the tree.
    pub name: String,
    /// The blob hash.
    pub hash: Hash,
    /// The file mode; `0` when unknown.
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

    /// Returns the file contents.
    ///
    /// # Errors
    ///
    /// Propagates blob-lookup failures.
    pub fn contents(&self) -> Result<Vec<u8>> {
        let blob = self.repo.lookup_blob(self.hash)?;
        Ok(blob.contents())
    }

    /// Returns the blob object for this file.
    ///
    /// # Errors
    ///
    /// Propagates blob-lookup failures.
    pub fn blob(&self) -> Result<crate::Blob<'repo>> {
        self.repo.lookup_blob(self.hash)
    }
}

/// An iterator over the files in a tree.
///
/// Holds a materialized list of files and yields them in order. Implements
/// [`cf_alg::PullIterator`]: [`next`](FileIter::next) returns
/// [`IteratorError::Eof`] when exhausted, and [`close`](FileIter::close) marks
/// the iterator drained.
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
    /// Convenience inherent method; returns `None` at exhaustion.
    #[must_use]
    pub fn next_file(&mut self) -> Option<File<'repo>> {
        if self.idx >= self.files.len() {
            return None;
        }
        let f = self.files[self.idx].clone();
        self.idx += 1;
        Some(f)
    }

    /// Calls `cb` for each remaining file.
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

    /// Marks the iterator drained.
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

//! Git signatures (author / committer).

/// A git signature: name, email, and a commit time.
///
/// The time is the libgit2 [`git2::Time`] (seconds since epoch + UTC offset in
/// minutes), so author/committer timestamps round-trip identically through the
/// report pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Author / committer name.
    pub name: String,
    /// Author / committer email.
    pub email: String,
    /// Commit time (seconds since the Unix epoch + UTC offset minutes).
    pub when: git2::Time,
}

impl Default for Signature {
    /// The zero signature: empty name/email and the epoch with a zero offset,
    /// as returned by test-double commits.
    fn default() -> Self {
        Signature {
            name: String::new(),
            email: String::new(),
            when: git2::Time::new(0, 0),
        }
    }
}

impl Signature {
    /// Borrows a libgit2 [`git2::Signature`] into the owned [`Signature`].
    #[must_use]
    pub(crate) fn from_git2(sig: &git2::Signature<'_>) -> Self {
        Signature {
            name: sig.name().unwrap_or_default().to_string(),
            email: sig.email().unwrap_or_default().to_string(),
            when: sig.when(),
        }
    }
}

//! Git data model shared between the plumbing providers.
//!
//! The Go implementation threads go-git's `object.Change` / `object.Changes`
//! (and `plumbing.Hash`, `object.Commit`) between providers. The design maps
//! go-git onto `git2` (libgit2). This module defines the small, owned,
//! pipeline-facing types that mirror the go-git vocabulary so providers can be
//! ported faithfully without exposing libgit2 lifetimes through the analyzer
//! map.
//!
//! These types are intentionally minimal: they carry exactly the fields the
//! plumbing providers read (`Change.Action()`, `Change.From/To.Name`,
//! `Change.*.TreeEntry.Hash`). When `cf-core` grows a canonical git model this
//! module should defer to it.

/// A 20-byte SHA-1 object id, mirroring go-git's `plumbing.Hash`.
///
/// Wrapped in a newtype so it can be used as a `HashMap` key (the blob cache is
/// keyed by hash) and rendered identically to go-git (`%x`, 40 lowercase hex).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Hash(pub [u8; 20]);

impl Hash {
    /// The zero hash, go-git's `plumbing.ZeroHash`.
    pub const ZERO: Hash = Hash([0u8; 20]);

    /// Whether this is the zero hash.
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 20]
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

impl From<git2::Oid> for Hash {
    fn from(oid: git2::Oid) -> Self {
        let bytes = oid.as_bytes();
        let mut h = [0u8; 20];
        // git2 oids are 20 bytes for SHA-1 repositories.
        let n = bytes.len().min(20);
        h[..n].copy_from_slice(&bytes[..n]);
        Hash(h)
    }
}

/// The kind of change to a path, mirroring go-git's `merkletrie.Action`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// File added (present in the new tree, absent in the old).
    Insert,
    /// File removed (present in the old tree, absent in the new).
    Delete,
    /// File content changed.
    Modify,
}

/// One side of a [`Change`], mirroring go-git's `object.ChangeEntry`.
///
/// Only the fields actually consulted by the plumbing providers are modelled:
/// the path `name` and the tree-entry `hash`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeEntry {
    /// Full path of the entry, e.g. `"src/main.rs"`.
    pub name: String,
    /// Object id of the blob. [`Hash::ZERO`] when the side is empty (an
    /// inserted file has a zero `from`, a deleted file a zero `to`).
    pub hash: Hash,
}

/// A single file change between two trees, mirroring go-git's `object.Change`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// State before the change. Empty (`name == ""`, `hash == ZERO`) for inserts.
    pub from: ChangeEntry,
    /// State after the change. Empty for deletes.
    pub to: ChangeEntry,
}

impl Change {
    /// Classify the change, mirroring go-git's `(*object.Change).Action()`.
    ///
    /// go-git returns an error when both sides are empty; that situation never
    /// arises for changes produced by the providers, but we model it as
    /// `None` rather than panicking.
    pub fn action(&self) -> Option<Action> {
        let from_empty = self.from.name.is_empty() && self.from.hash.is_zero();
        let to_empty = self.to.name.is_empty() && self.to.hash.is_zero();
        match (from_empty, to_empty) {
            (true, true) => None,
            (true, false) => Some(Action::Insert),
            (false, true) => Some(Action::Delete),
            (false, false) => Some(Action::Modify),
        }
    }
}

/// An ordered list of changes, mirroring go-git's `object.Changes`.
pub type Changes = Vec<Change>;

/// The minimal commit metadata read by the providers (the author signature).
///
/// Mirrors the fields of go-git's `object.Commit.Author` used here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Author display name.
    pub name: String,
    /// Author email.
    pub email: String,
    /// Authoring time as a UTC unix timestamp in seconds.
    ///
    /// go-git stores a `time.Time`; the providers only ever subtract two such
    /// times and divide by a tick size, so a monotonic seconds count preserves
    /// behavior exactly while staying serialization-agnostic.
    pub when_unix: i64,
}

/// The minimal commit modelled here, mirroring gitlib's `Commit`.
///
/// Only the fields the plumbing providers consult are modelled: the author
/// signature ([`crate::identity_detector::IdentityDetector`]
/// reads `Author()`) and the committer signature
/// ([`crate::ticks::TicksSinceStart`] reads
/// `Committer().When`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Author signature (name, email, authoring time).
    pub author: Signature,
    /// Committer signature (name, email, commit time).
    pub committer: Signature,
}

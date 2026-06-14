//! Git data model shared between the plumbing providers.
//!
//! Small, owned, pipeline-facing types so providers do not expose libgit2
//! lifetimes through the analyzer map.
//!
//! These types are intentionally minimal: they carry exactly the fields the
//! plumbing providers read (the change action, the per-side path names, and
//! the per-side blob hashes). When the workspace grows a canonical git model
//! this module should defer to it.

/// A 20-byte SHA-1 object id.
///
/// Wrapped in a newtype so it can be used as a `HashMap` key (the blob cache
/// is keyed by hash); [`Display`](std::fmt::Display) renders 40 lowercase hex
/// digits (the report-facing hash format).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Hash(pub [u8; 20]);

impl Hash {
    /// The all-zero hash (an absent side of a change).
    pub const ZERO: Hash = Hash([0u8; 20]);

    /// Whether this is the zero hash.
    #[must_use]
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

/// The kind of change to a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// File added (present in the new tree, absent in the old).
    Insert,
    /// File removed (present in the old tree, absent in the new).
    Delete,
    /// File content changed.
    Modify,
}

/// One side of a [`Change`].
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

/// A single file change between two trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// State before the change. Empty (`name == ""`, `hash == ZERO`) for inserts.
    pub from: ChangeEntry,
    /// State after the change. Empty for deletes.
    pub to: ChangeEntry,
}

impl Change {
    /// Classify the change from which sides are present.
    ///
    /// Both sides empty never arises for changes produced by the providers;
    /// it is modelled as `None` rather than panicking.
    #[must_use]
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

/// An ordered list of changes.
pub type Changes = Vec<Change>;

/// An author/committer signature — the minimal commit metadata read by the
/// providers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Author display name.
    pub name: String,
    /// Author email.
    pub email: String,
    /// Authoring time as a UTC unix timestamp in seconds.
    ///
    /// The providers only ever subtract two such times and divide by a tick
    /// size, so a seconds count preserves behavior exactly while staying
    /// serialization-agnostic.
    pub when_unix: i64,
}

/// The minimal commit modelled here.
///
/// Only the fields the plumbing providers consult are modelled: the author
/// signature (read by [`crate::identity_detector::IdentityDetector`]) and the
/// committer signature (read by [`crate::ticks::TicksSinceStart`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Author signature (name, email, authoring time).
    pub author: Signature,
    /// Committer signature (name, email, commit time).
    pub committer: Signature,
}

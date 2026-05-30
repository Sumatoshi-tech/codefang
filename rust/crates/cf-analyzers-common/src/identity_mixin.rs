//! Identity-resolution mixin (`identity_mixin.go`).
//!
//! Deduplicates the two-tier reversed-people-dict resolution shared by the
//! burndown, couples, imports, and devs history analyzers: prefer the
//! pipeline-supplied [`IdentityDetector`]'s dict when present and non-empty,
//! otherwise fall back to a manually-set dict.
//!
//! The authoritative `IdentityDetector` lives in
//! `internal/analyzers/plumbing` (the `cf-analyzers-plumbing` crate). The
//! minimal shape used here — a struct with a `reversed_people_dict` field — is
//! defined locally as [`IdentityDetector`] so this crate's surface does not
//! couple to that crate's evolving representation; consolidating onto the
//! `cf-analyzers-plumbing` definition is tracked in the crate-level roadmap
//! note in `lib.rs`.

/// Minimal stand-in for `plumbing.IdentityDetector`.
///
/// Only the `reversed_people_dict` field is observed by the mixin.
#[derive(Debug, Clone, Default)]
pub struct IdentityDetector {
    /// People dictionary indexed by resolved identity id.
    pub reversed_people_dict: Vec<String>,
}

/// Two-tier reversed-people-dict resolver.
///
/// Mirrors `common.IdentityMixin`. The optional [`IdentityDetector`] reference
/// is set by the pipeline; the fallback dict is set from Configure facts.
#[derive(Debug, Clone, Default)]
pub struct IdentityMixin {
    /// Pipeline-supplied identity detector, when available.
    pub identity: Option<IdentityDetector>,
    /// Fallback reversed-people dictionary.
    pub reversed_people_dict: Vec<String>,
}

impl IdentityMixin {
    /// Returns the identity-resolved people dictionary.
    ///
    /// Prefers the [`IdentityDetector`]'s dict when present and non-empty,
    /// otherwise returns the manually-set fallback. Mirrors
    /// `common.IdentityMixin.GetReversedPeopleDict`.
    #[must_use]
    pub fn get_reversed_people_dict(&self) -> &[String] {
        if let Some(id) = &self.identity {
            if !id.reversed_people_dict.is_empty() {
                return &id.reversed_people_dict;
            }
        }
        &self.reversed_people_dict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_only() {
        let m = IdentityMixin {
            identity: None,
            reversed_people_dict: vec!["alice".into(), "bob".into()],
        };
        assert_eq!(m.get_reversed_people_dict(), &["alice", "bob"]);
    }

    #[test]
    fn prefer_identity() {
        let m = IdentityMixin {
            identity: Some(IdentityDetector {
                reversed_people_dict: vec!["carol".into(), "dave".into()],
            }),
            reversed_people_dict: vec!["alice".into(), "bob".into()],
        };
        assert_eq!(m.get_reversed_people_dict(), &["carol", "dave"]);
    }

    #[test]
    fn empty_identity_falls_back() {
        let m = IdentityMixin {
            identity: Some(IdentityDetector {
                reversed_people_dict: vec![],
            }),
            reversed_people_dict: vec!["alice".into()],
        };
        assert_eq!(m.get_reversed_people_dict(), &["alice"]);
    }
}

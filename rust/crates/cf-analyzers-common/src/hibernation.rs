//! No-op hibernation mixin (`no_state_hibernation.go`).
//!
//! Analyzers that accumulate no working state between streaming chunks embed
//! [`NoStateHibernation`] to get no-op `hibernate`/`boot` implementations.
//!
//! The `streaming.Hibernatable` interface lives in the not-yet-ported
//! `internal/streaming` package (`cf-streaming` crate); its minimal shape is
//! defined here as [`Hibernatable`]. Consolidating onto the `cf-streaming`
//! definition is tracked in the crate-level roadmap note in `lib.rs`.

/// A unit error type for hibernation operations.
///
/// The Go methods return `error`; the no-op implementations always return
/// success, so callers will only ever see [`Result::Ok`] from
/// [`NoStateHibernation`].
pub type HibernateError = std::io::Error;

/// Trait for state that can be hibernated to / restored from a dormant form
/// between streaming chunks. Mirrors `streaming.Hibernatable`.
pub trait Hibernatable {
    /// Releases or persists working state. Returns an error on failure.
    ///
    /// # Errors
    /// Returns an error if the underlying state cannot be hibernated.
    fn hibernate(&mut self) -> Result<(), HibernateError>;

    /// Restores working state previously released by [`Hibernatable::hibernate`].
    ///
    /// # Errors
    /// Returns an error if the underlying state cannot be restored.
    fn boot(&mut self) -> Result<(), HibernateError>;
}

/// A zero-size mixin providing no-op [`Hibernatable`] implementations.
///
/// Mirrors `common.NoStateHibernation`.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoStateHibernation;

impl Hibernatable for NoStateHibernation {
    /// No-op. Always succeeds.
    fn hibernate(&mut self) -> Result<(), HibernateError> {
        Ok(())
    }

    /// No-op. Always succeeds.
    fn boot(&mut self) -> Result<(), HibernateError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hibernate_is_noop() {
        let mut h = NoStateHibernation;
        assert!(h.hibernate().is_ok());
    }

    #[test]
    fn boot_is_noop() {
        let mut h = NoStateHibernation;
        assert!(h.boot().is_ok());
    }
}

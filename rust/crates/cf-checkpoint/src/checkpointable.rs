//! The [`Checkpointable`] trait.

use crate::error::Result;

/// Optional interface for analyzers that support checkpointing.
///
/// Ported from Go's `checkpoint.Checkpointable`:
///
/// ```go
/// type Checkpointable interface {
///     SaveCheckpoint(dir string) error
///     LoadCheckpoint(dir string) error
///     CheckpointSize() int64
/// }
/// ```
///
/// An analyzer that implements this trait can have its state snapshotted to a
/// directory by the [`Manager`](crate::Manager) and restored later for crash
/// recovery / incremental resume. The on-disk layout written by
/// [`save_checkpoint`](Checkpointable::save_checkpoint) is private to each
/// implementor; the manager only allocates a per-analyzer directory and calls
/// the trait methods.
pub trait Checkpointable {
    /// Writes analyzer state into `dir`.
    ///
    /// `dir` is guaranteed to exist when the manager calls this. The
    /// implementor chooses its own file names within `dir`.
    fn save_checkpoint(&self, dir: &std::path::Path) -> Result<()>;

    /// Restores analyzer state from `dir`, previously written by
    /// [`save_checkpoint`](Checkpointable::save_checkpoint).
    fn load_checkpoint(&mut self, dir: &std::path::Path) -> Result<()>;

    /// Returns the estimated size of the checkpoint in bytes.
    fn checkpoint_size(&self) -> i64;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Mirrors checkpointable_test.go's `mockCheckpointable`.
    struct MockCheckpointable {
        data: String,
    }

    impl Checkpointable for MockCheckpointable {
        fn save_checkpoint(&self, dir: &Path) -> Result<()> {
            std::fs::write(dir.join("mock.bin"), self.data.as_bytes())?;
            Ok(())
        }

        fn load_checkpoint(&mut self, dir: &Path) -> Result<()> {
            let data = std::fs::read(dir.join("mock.bin"))?;
            self.data = String::from_utf8_lossy(&data).into_owned();
            Ok(())
        }

        fn checkpoint_size(&self) -> i64 {
            self.data.len() as i64
        }
    }

    // Ported from TestCheckpointable_Interface (compile-time trait object check).
    #[test]
    fn implements_checkpointable() {
        let m = MockCheckpointable {
            data: String::new(),
        };
        let _: &dyn Checkpointable = &m;
    }

    // Ported from TestCheckpointable_SaveLoad.
    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let original = MockCheckpointable {
            data: "test state data".into(),
        };
        original.save_checkpoint(dir.path()).unwrap();

        let mut restored = MockCheckpointable {
            data: String::new(),
        };
        restored.load_checkpoint(dir.path()).unwrap();
        assert_eq!(original.data, restored.data);
    }

    // Ported from TestCheckpointable_Size.
    #[test]
    fn size_reports_byte_length() {
        let m = MockCheckpointable {
            data: "12345".into(),
        };
        assert_eq!(m.checkpoint_size(), 5);
    }
}

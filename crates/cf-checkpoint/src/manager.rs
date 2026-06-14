//! The checkpoint [`Manager`], coordinating checkpoints across analyzers.
//!
//! The manager owns the on-disk layout:
//!
//! ```text
//! <base_dir>/<repo_hash>/
//!   checkpoint.json          # Metadata (atomic JSON write)
//!   analyzer_0/...           # per-Checkpointable state (one dir per analyzer)
//!   analyzer_1/...
//! ```

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::checkpointable::Checkpointable;
use crate::codec::{self, JsonCodec};
use crate::error::{CheckpointError, Result};
use crate::state::{Metadata, StreamingState};

/// Current checkpoint metadata format version.
///
/// Bumped from 1 to 2 when aggregator spill state was added.
pub const METADATA_VERSION: i64 = 2;

/// File basename for checkpoint metadata (without extension); the file is
/// `checkpoint.json`.
const METADATA_BASENAME: &str = "checkpoint";

/// Seconds in one calendar week, used to express [`DEFAULT_MAX_AGE`].
const WEEK_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Default maximum checkpoint age before it is considered stale: one week.
pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(WEEK_SECONDS);

/// Default maximum total checkpoint size: 1 GiB.
pub const DEFAULT_MAX_SIZE: i64 = 1 << 30;

/// Directory permissions for checkpoints (`0o750`), applied on Unix.
#[cfg(unix)]
const DIR_PERM: u32 = 0o750;

/// Returns the default checkpoint directory: `~/.codefang/checkpoints`.
///
/// If the user's home directory cannot be resolved, the base falls back to
/// `.` (current directory).
#[must_use]
pub fn default_dir() -> PathBuf {
    let home = home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".codefang").join("checkpoints")
}

/// Computes a short hash of the repository path for use as a directory name.
///
/// SHA-256 of the path bytes, hex-encoding the first 8 bytes (16 lowercase hex
/// characters). The derivation is part of the on-disk layout contract and is
/// locked by a known-answer test below.
#[must_use]
pub fn repo_hash(repo_path: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(repo_path.as_bytes());
    // First 8 bytes -> 16 hex chars.
    let mut out = String::with_capacity(16);
    for &byte in &digest[..8] {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Coordinates checkpoints across analyzers for one repository.
///
/// Construct with [`Manager::new`]; the retention fields
/// ([`max_age`](Manager::max_age), [`max_size`](Manager::max_size)) are
/// initialized to the defaults and are publicly mutable.
#[derive(Debug, Clone)]
pub struct Manager {
    /// Base directory containing one subdirectory per repository hash.
    pub base_dir: PathBuf,
    /// Short repository hash identifying this repo's checkpoint subdirectory.
    pub repo_hash: String,
    /// Maximum checkpoint age before it is considered stale.
    pub max_age: Duration,
    /// Maximum total checkpoint size in bytes.
    pub max_size: i64,
}

impl Manager {
    /// Creates a new checkpoint manager.
    ///
    /// Retention defaults to [`DEFAULT_MAX_AGE`] / [`DEFAULT_MAX_SIZE`].
    pub fn new(base_dir: impl Into<PathBuf>, repo_hash: impl Into<String>) -> Self {
        Self {
            base_dir: base_dir.into(),
            repo_hash: repo_hash.into(),
            max_age: DEFAULT_MAX_AGE,
            max_size: DEFAULT_MAX_SIZE,
        }
    }

    /// Returns the directory for this repository's checkpoint
    /// (`base_dir/repo_hash`).
    #[must_use]
    pub fn checkpoint_dir(&self) -> PathBuf {
        self.base_dir.join(&self.repo_hash)
    }

    /// Returns the path to the metadata file (`<checkpoint_dir>/checkpoint.json`).
    #[must_use]
    pub fn metadata_path(&self) -> PathBuf {
        self.checkpoint_dir()
            .join(format!("{METADATA_BASENAME}.json"))
    }

    /// Returns `true` if a checkpoint metadata file exists.
    ///
    /// A plain existence check on the metadata path; it does not validate the
    /// contents.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.metadata_path().exists()
    }

    /// Removes the checkpoint for the current repository.
    ///
    /// Returns `Ok(())` (no error) when the directory does not exist.
    ///
    /// # Errors
    ///
    /// Returns a [`CheckpointError`] if the directory cannot be inspected or
    /// removed.
    pub fn clear(&self) -> Result<()> {
        let cp_dir = self.checkpoint_dir();
        match std::fs::metadata(&cp_dir) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(CheckpointError::Io(e)),
        }
        std::fs::remove_dir_all(&cp_dir)
            .map_err(|e| CheckpointError::Codec(format!("remove checkpoint dir: {e}")))?;
        Ok(())
    }

    /// Creates a checkpoint for all checkpointable analyzers.
    ///
    /// For each analyzer in `checkpointables` a directory `analyzer_<i>` is
    /// created and the analyzer's
    /// [`save_checkpoint`](Checkpointable::save_checkpoint) is invoked, then
    /// the [`Metadata`] is written atomically as `checkpoint.json`.
    ///
    /// `created_at` is supplied by the caller so the wall clock can be pinned
    /// in tests/goldens (DESIGN §2.8); use [`Manager::save_now`] for the
    /// production behavior (current UTC time, RFC3339).
    ///
    /// # Errors
    ///
    /// Returns a [`CheckpointError`] if a directory cannot be created, an
    /// analyzer fails to save, or the metadata write fails.
    pub fn save(
        &self,
        checkpointables: &mut [&mut dyn Checkpointable],
        state: StreamingState,
        repo_path: &str,
        analyzer_names: &[String],
        created_at: String,
    ) -> Result<()> {
        let cp_dir = self.checkpoint_dir();
        create_dir_all_perm(&cp_dir)
            .map_err(|e| CheckpointError::Codec(format!("create checkpoint dir: {e}")))?;

        // Save each checkpointable analyzer into analyzer_<i>.
        for (i, cp) in checkpointables.iter_mut().enumerate() {
            let analyzer_dir = cp_dir.join(format!("analyzer_{i}"));
            create_dir_all_perm(&analyzer_dir)
                .map_err(|e| CheckpointError::Codec(format!("create analyzer dir: {e}")))?;
            cp.save_checkpoint(&analyzer_dir).map_err(|e| {
                CheckpointError::Codec(format!("save checkpoint for analyzer {i}: {e}"))
            })?;
        }

        let meta = Metadata {
            version: METADATA_VERSION,
            repo_path: repo_path.to_string(),
            repo_hash: self.repo_hash.clone(),
            created_at,
            analyzers: analyzer_names.to_vec(),
            streaming_state: state,
            // An empty map serializes to `{}` (not null) — metadata-layout
            // contract.
            checksums: std::collections::BTreeMap::new(),
        };

        codec::save_state(&cp_dir, METADATA_BASENAME, &JsonCodec::new(), &meta)
            .map_err(|e| CheckpointError::Codec(format!("save metadata: {e}")))?;
        Ok(())
    }

    /// Convenience wrapper over [`Manager::save`] that stamps `created_at`
    /// with the current UTC time formatted as RFC3339. Prefer
    /// [`Manager::save`] with an injected timestamp in tests so output is
    /// deterministic.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`Manager::save`].
    pub fn save_now(
        &self,
        checkpointables: &mut [&mut dyn Checkpointable],
        state: StreamingState,
        repo_path: &str,
        analyzer_names: &[String],
    ) -> Result<()> {
        self.save(
            checkpointables,
            state,
            repo_path,
            analyzer_names,
            now_rfc3339_utc(),
        )
    }

    /// Loads the checkpoint metadata.
    ///
    /// # Errors
    ///
    /// Returns [`CheckpointError::Codec`] if the metadata file cannot be read
    /// or parsed.
    pub fn load_metadata(&self) -> Result<Metadata> {
        codec::load_state(&self.checkpoint_dir(), METADATA_BASENAME, &JsonCodec::new())
            .map_err(|e| CheckpointError::Codec(format!("load metadata: {e}")))
    }

    /// Restores state for all checkpointable analyzers and returns the saved
    /// [`StreamingState`].
    ///
    /// Loads metadata first, then calls
    /// [`load_checkpoint`](Checkpointable::load_checkpoint) on each analyzer
    /// from its `analyzer_<i>` directory.
    ///
    /// # Errors
    ///
    /// Returns a [`CheckpointError`] if the metadata or any analyzer state
    /// cannot be restored.
    pub fn load(&self, checkpointables: &mut [&mut dyn Checkpointable]) -> Result<StreamingState> {
        let meta = self.load_metadata()?;
        let cp_dir = self.checkpoint_dir();

        for (i, cp) in checkpointables.iter_mut().enumerate() {
            let analyzer_dir = cp_dir.join(format!("analyzer_{i}"));
            cp.load_checkpoint(&analyzer_dir).map_err(|e| {
                CheckpointError::Codec(format!("load checkpoint for analyzer {i}: {e}"))
            })?;
        }

        Ok(meta.streaming_state)
    }

    /// Checks whether the stored checkpoint is valid for the given parameters.
    ///
    /// # Errors
    ///
    /// * [`CheckpointError::VersionMismatch`] if the stored version differs
    ///   from [`METADATA_VERSION`],
    /// * [`CheckpointError::RepoPathMismatch`] if `repo_path` differs,
    /// * [`CheckpointError::AnalyzerMismatch`] if `analyzer_names` differs
    ///   (order-sensitive),
    /// * any [`Manager::load_metadata`] error.
    pub fn validate(&self, repo_path: &str, analyzer_names: &[String]) -> Result<()> {
        let meta = self.load_metadata()?;

        if meta.version != METADATA_VERSION {
            return Err(CheckpointError::VersionMismatch {
                found: meta.version,
                current: METADATA_VERSION,
            });
        }

        if meta.repo_path != repo_path {
            return Err(CheckpointError::RepoPathMismatch {
                want: meta.repo_path,
                got: repo_path.to_string(),
            });
        }

        if meta.analyzers.as_slice() != analyzer_names {
            return Err(CheckpointError::AnalyzerMismatch {
                want: meta.analyzers,
                got: analyzer_names.to_vec(),
            });
        }

        Ok(())
    }
}

/// Creates a directory and all parents with `0o750` permissions on Unix.
fn create_dir_all_perm(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs::DirBuilder;
        use std::os::unix::fs::DirBuilderExt;
        DirBuilder::new().recursive(true).mode(DIR_PERM).create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Returns the user's home directory, or `None` if it cannot be resolved:
/// `$HOME` on Unix, `%USERPROFILE%` on Windows (no extra dependency needed).
fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    }
}

/// Formats the current system time as an RFC3339 UTC string (`...Z`) at
/// second precision.
///
/// Implemented without `chrono` (DESIGN §2.8 cautions against its formatter
/// for byte-identity). For deterministic output prefer [`Manager::save`] with
/// an injected timestamp; this helper backs the production default only.
fn now_rfc3339_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_unix_rfc3339(now.as_secs() as i64)
}

/// Converts a Unix timestamp (seconds) to an RFC3339 UTC string with `Z`
/// suffix and no fractional part, e.g. `2026-02-05T12:00:00Z` (civil-time
/// conversion on the proleptic Gregorian calendar).
fn format_unix_rfc3339(secs: i64) -> String {
    let total_days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let (year, month, day) = civil_from_days(total_days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Converts a day-index relative to the Unix epoch into a `(year, month, day)`
/// civil date using Howard Hinnant's well-known algorithm (proleptic Gregorian
/// calendar).
const fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146_096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AggregatorSpillEntry;
    use std::cell::RefCell;
    use std::path::Path;

    const FIXED_TIME: &str = "2026-02-05T12:00:00Z";

    fn names(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    // Mirrors the reference suite's `mockCheckpointable`.
    struct MockCheckpointable {
        data: RefCell<String>,
    }

    impl MockCheckpointable {
        fn new(data: &str) -> Self {
            Self {
                data: RefCell::new(data.to_string()),
            }
        }
    }

    impl Checkpointable for MockCheckpointable {
        fn save_checkpoint(&self, dir: &Path) -> Result<()> {
            std::fs::write(dir.join("mock.bin"), self.data.borrow().as_bytes())?;
            Ok(())
        }

        fn load_checkpoint(&mut self, dir: &Path) -> Result<()> {
            let data = std::fs::read(dir.join("mock.bin"))?;
            *self.data.borrow_mut() = String::from_utf8_lossy(&data).into_owned();
            Ok(())
        }

        fn checkpoint_size(&self) -> i64 {
            self.data.borrow().len() as i64
        }
    }

    // Mirrors TestManager_New.
    #[test]
    fn manager_new() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert_eq!(m.base_dir, dir.path());
        assert_eq!(m.repo_hash, "abc123");
        assert_eq!(m.max_age, DEFAULT_MAX_AGE);
        assert_eq!(m.max_size, DEFAULT_MAX_SIZE);
    }

    // Mirrors TestManager_CheckpointDir.
    #[test]
    fn manager_checkpoint_dir() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert_eq!(m.checkpoint_dir(), dir.path().join("abc123"));
    }

    // Mirrors TestManager_MetadataPath.
    #[test]
    fn manager_metadata_path() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert_eq!(
            m.metadata_path(),
            dir.path().join("abc123").join("checkpoint.json")
        );
    }

    // Mirrors TestManager_Exists_NoCheckpoint.
    #[test]
    fn manager_exists_no_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert!(!m.exists());
    }

    // Mirrors TestManager_Exists_WithCheckpoint.
    #[test]
    fn manager_exists_with_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        std::fs::create_dir_all(m.checkpoint_dir()).unwrap();
        std::fs::write(m.metadata_path(), br#"{"version":1}"#).unwrap();
        assert!(m.exists());
    }

    // Mirrors TestManager_Clear.
    #[test]
    fn manager_clear() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        std::fs::create_dir_all(m.checkpoint_dir()).unwrap();
        std::fs::write(m.metadata_path(), br#"{"version":1}"#).unwrap();
        assert!(m.exists());
        m.clear().unwrap();
        assert!(!m.exists());
    }

    // Mirrors TestManager_Clear_NonExistent.
    #[test]
    fn manager_clear_non_existent() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert!(m.clear().is_ok());
    }

    // Mirrors TestManager_SaveLoad_Metadata.
    #[test]
    fn manager_save_load_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        let state = StreamingState {
            total_commits: 100_000,
            processed_commits: 50_000,
            current_chunk: 1,
            total_chunks: 2,
            last_commit_hash: "def456".into(),
            last_tick: 42,
            aggregator_spills: Vec::new(),
        };
        m.save(
            &mut [],
            state.clone(),
            "/path/to/repo",
            &names(&["burndown"]),
            FIXED_TIME.into(),
        )
        .unwrap();
        assert!(m.exists());

        let meta = m.load_metadata().unwrap();
        assert_eq!(meta.version, METADATA_VERSION);
        assert_eq!(meta.repo_path, "/path/to/repo");
        assert_eq!(meta.repo_hash, "abc123");
        assert_eq!(meta.analyzers, names(&["burndown"]));
        assert_eq!(meta.streaming_state.total_commits, state.total_commits);
        assert_eq!(
            meta.streaming_state.processed_commits,
            state.processed_commits
        );
    }

    // Mirrors TestManager_SaveLoad_Checkpointables.
    #[test]
    fn manager_save_load_checkpointables() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        let state = StreamingState {
            total_commits: 100,
            processed_commits: 50,
            ..Default::default()
        };

        let mut original = MockCheckpointable::new("analyzer state");
        m.save(
            &mut [&mut original],
            state.clone(),
            "/path/to/repo",
            &names(&["mock"]),
            FIXED_TIME.into(),
        )
        .unwrap();

        let mut restored = MockCheckpointable::new("");
        let loaded_state = m.load(&mut [&mut restored]).unwrap();
        assert_eq!(*restored.data.borrow(), "analyzer state");
        assert_eq!(loaded_state.total_commits, state.total_commits);
        assert_eq!(loaded_state.processed_commits, state.processed_commits);
    }

    // Mirrors TestManager_DefaultValues.
    #[test]
    fn manager_default_values() {
        assert_eq!(DEFAULT_MAX_AGE, Duration::from_secs(WEEK_SECONDS));
        assert_eq!(DEFAULT_MAX_SIZE, 1 << 30);
    }

    // Mirrors TestManager_Validate_Success.
    #[test]
    fn manager_validate_success() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        let state = StreamingState {
            total_commits: 100,
            processed_commits: 50,
            last_commit_hash: "def456".into(),
            ..Default::default()
        };
        m.save(
            &mut [],
            state,
            "/path/to/repo",
            &names(&["burndown"]),
            FIXED_TIME.into(),
        )
        .unwrap();
        assert!(m.validate("/path/to/repo", &names(&["burndown"])).is_ok());
    }

    // Mirrors TestManager_Validate_WrongRepo.
    #[test]
    fn manager_validate_wrong_repo() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        m.save(
            &mut [],
            StreamingState::default(),
            "/path/to/repo",
            &names(&["burndown"]),
            FIXED_TIME.into(),
        )
        .unwrap();
        let err = m
            .validate("/different/repo", &names(&["burndown"]))
            .unwrap_err();
        assert!(matches!(err, CheckpointError::RepoPathMismatch { .. }));
    }

    // Mirrors TestManager_Validate_WrongAnalyzers.
    #[test]
    fn manager_validate_wrong_analyzers() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        m.save(
            &mut [],
            StreamingState::default(),
            "/path/to/repo",
            &names(&["burndown"]),
            FIXED_TIME.into(),
        )
        .unwrap();
        let err = m.validate("/path/to/repo", &names(&["devs"])).unwrap_err();
        assert!(matches!(err, CheckpointError::AnalyzerMismatch { .. }));
    }

    // Mirrors TestManager_Validate_NoCheckpoint.
    #[test]
    fn manager_validate_no_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert!(m.validate("/path/to/repo", &names(&["burndown"])).is_err());
    }

    // Mirrors TestDefaultDir.
    #[test]
    fn default_dir_contains_codefang_checkpoints() {
        let dir = default_dir();
        let s = dir.to_string_lossy();
        assert!(s.contains(".codefang"));
        assert!(s.contains("checkpoints"));
    }

    // Mirrors TestRepoHash.
    #[test]
    fn repo_hash_is_16_chars_and_stable() {
        let hash = repo_hash("/path/to/repo");
        assert_eq!(hash.len(), 16);
        assert_eq!(hash, repo_hash("/path/to/repo"));
        assert_ne!(hash, repo_hash("/different/repo"));
    }

    #[test]
    fn repo_hash_known_answer() {
        // Independently computed expected value:
        // hex(sha256("/path/to/repo")[:8]).
        let full = Sha256::digest(b"/path/to/repo");
        let expected: String = full[..8].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(repo_hash("/path/to/repo"), expected);
    }

    // Mirrors TestManager_Validate_OldVersion.
    #[test]
    fn manager_validate_old_version() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        std::fs::create_dir_all(m.checkpoint_dir()).unwrap();
        let meta = br#"{"version":1,"repo_path":"/test/repo","analyzers":["burndown"]}"#;
        std::fs::write(m.metadata_path(), meta).unwrap();
        let err = m.validate("/test/repo", &names(&["burndown"])).unwrap_err();
        assert!(matches!(err, CheckpointError::VersionMismatch { .. }));
    }

    // Mirrors TestManager_SaveLoad_AggregatorSpills.
    #[test]
    fn manager_save_load_aggregator_spills() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        let state = StreamingState {
            total_commits: 100,
            processed_commits: 50,
            current_chunk: 1,
            total_chunks: 2,
            aggregator_spills: vec![
                AggregatorSpillEntry::default(),
                AggregatorSpillEntry {
                    dir: "/tmp/spill-1".into(),
                    count: 3,
                },
                AggregatorSpillEntry {
                    dir: "/tmp/spill-2".into(),
                    count: 1,
                },
            ],
            ..Default::default()
        };
        m.save(
            &mut [],
            state,
            "/test/repo",
            &names(&["burndown"]),
            FIXED_TIME.into(),
        )
        .unwrap();

        let meta = m.load_metadata().unwrap();
        assert_eq!(meta.version, METADATA_VERSION);
        let spills = &meta.streaming_state.aggregator_spills;
        assert_eq!(spills.len(), 3);
        assert!(spills[0].dir.is_empty());
        assert_eq!(spills[1].dir, "/tmp/spill-1");
        assert_eq!(spills[1].count, 3);
        assert_eq!(spills[2].dir, "/tmp/spill-2");
        assert_eq!(spills[2].count, 1);
    }

    // Mirrors TestManager_Save_ErrorOnMkdir.
    #[test]
    fn manager_save_error_on_mkdir() {
        let dir = tempfile::tempdir().unwrap();
        // Create a regular file, then point the manager base at it so the
        // directory creation inside that file path fails.
        let file_path = dir.path().join("checkpoint-test-file");
        std::fs::write(&file_path, b"x").unwrap();
        let m = Manager::new(&file_path, "abc123");
        let err = m.save(
            &mut [],
            StreamingState::default(),
            "/repo",
            &[],
            FIXED_TIME.into(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn rfc3339_formatter_matches_known_instant() {
        // Epoch maps to 1970-01-01T00:00:00Z.
        assert_eq!(format_unix_rfc3339(0), "1970-01-01T00:00:00Z");
        // A fixed civil instant round-trips to the canonical RFC3339 string.
        let secs = unix_secs(2026, 2, 5, 12, 0, 0);
        assert_eq!(format_unix_rfc3339(secs), "2026-02-05T12:00:00Z");
    }

    /// Test helper: civil date+time -> unix seconds (companion of
    /// `civil_from_days`). Uses the days-from-civil algorithm.
    fn unix_secs(y: i64, m: i64, d: i64, hh: i64, mm: i64, ss: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let day_index = era * 146_097 + doe - 719_468;
        day_index * 86_400 + hh * 3600 + mm * 60 + ss
    }
}

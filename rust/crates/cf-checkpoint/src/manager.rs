//! The checkpoint [`Manager`], coordinating checkpoints across analyzers.
//!
//! Ported from `internal/checkpoint/manager.go`. The manager owns the on-disk
//! layout:
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
/// Bumped from 1 to 2 when aggregator spill state was added (Go's
/// `MetadataVersion`).
pub const METADATA_VERSION: i64 = 2;

/// File basename for checkpoint metadata (without extension); the file is
/// `checkpoint.json`.
const METADATA_BASENAME: &str = "checkpoint";

/// Seconds in one calendar week, used to express [`DEFAULT_MAX_AGE`].
const WEEK_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Default maximum checkpoint age before it is considered stale: one week.
///
/// Mirrors Go's `DefaultMaxAge = 7 * 24 * time.Hour`.
pub const DEFAULT_MAX_AGE: Duration = Duration::from_secs(WEEK_SECONDS);

/// Default maximum total checkpoint size: 1 GiB.
///
/// Mirrors Go's `DefaultMaxSize = 1 << 30`.
pub const DEFAULT_MAX_SIZE: i64 = 1 << 30;

/// Directory permissions for checkpoints (`0o750`), applied on Unix.
#[cfg(unix)]
const DIR_PERM: u32 = 0o750;

/// Returns the default checkpoint directory: `~/.codefang/checkpoints`.
///
/// Ported from Go's `DefaultDir`. If the user's home directory cannot be
/// resolved, the base falls back to `.` (current directory), matching the Go
/// fallback `home = "."`.
#[must_use]
pub fn default_dir() -> PathBuf {
    let home = home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".codefang").join("checkpoints")
}

/// Computes a short hash of the repository path for use as a directory name.
///
/// Ported from Go's `RepoHash`: SHA-256 of the path bytes, hex-encoding the
/// first 8 bytes (16 hex characters). The output is byte-identical to the Go
/// implementation for the same input path.
#[must_use]
pub fn repo_hash(repo_path: &str) -> String {
    let digest = Sha256::digest(repo_path.as_bytes());
    // First 8 bytes -> 16 hex chars.
    let mut out = String::with_capacity(16);
    for byte in &digest[..8] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Coordinates checkpoints across analyzers for one repository.
///
/// Ported from Go's `checkpoint.Manager`. Construct with [`Manager::new`]; the
/// retention fields ([`max_age`](Manager::max_age),
/// [`max_size`](Manager::max_size)) are initialized to the defaults and are
/// publicly mutable, matching the Go exported struct fields.
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
    /// Creates a new checkpoint manager (Go's `NewManager`).
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
    /// Ported from Go's `Exists`, which is a plain `os.Stat` on the metadata
    /// path (it does not validate the contents).
    #[must_use]
    pub fn exists(&self) -> bool {
        self.metadata_path().exists()
    }

    /// Removes the checkpoint for the current repository.
    ///
    /// Ported from Go's `Clear`. Returns `Ok(())` (no error) when the directory
    /// does not exist, matching `os.IsNotExist` handling.
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
    /// Ported from Go's `Save`. For each analyzer in `checkpointables` a
    /// directory `analyzer_<i>` is created and the analyzer's
    /// [`save_checkpoint`](Checkpointable::save_checkpoint) is invoked, then the
    /// [`Metadata`] is written atomically as `checkpoint.json`.
    ///
    /// `created_at` is supplied by the caller so the wall clock can be pinned in
    /// tests/goldens (DESIGN §2.8); the Go code stamps this with
    /// `time.Now().UTC().Format(time.RFC3339)`. Use [`Manager::save_now`] for
    /// the production behavior.
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
            // Go initializes an empty (non-nil) map here; an empty BTreeMap
            // serializes to `{}` exactly as Go's `make(map[string]string)` does.
            checksums: std::collections::BTreeMap::new(),
        };

        codec::save_state(&cp_dir, METADATA_BASENAME, &JsonCodec::new(), &meta)
            .map_err(|e| CheckpointError::Codec(format!("save metadata: {e}")))?;
        Ok(())
    }

    /// Convenience wrapper over [`Manager::save`] that stamps `created_at` with
    /// the current UTC time formatted as RFC3339, matching Go's production
    /// `Save` exactly. Prefer [`Manager::save`] with an injected timestamp in
    /// tests so output is deterministic.
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
    /// Ported from Go's `LoadMetadata`.
    pub fn load_metadata(&self) -> Result<Metadata> {
        codec::load_state(&self.checkpoint_dir(), METADATA_BASENAME, &JsonCodec::new())
            .map_err(|e| CheckpointError::Codec(format!("load metadata: {e}")))
    }

    /// Restores state for all checkpointable analyzers and returns the saved
    /// [`StreamingState`].
    ///
    /// Ported from Go's `Load`. Loads metadata first, then calls
    /// [`load_checkpoint`](Checkpointable::load_checkpoint) on each analyzer
    /// from its `analyzer_<i>` directory.
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
    /// Ported from Go's `Validate`. Returns:
    /// * [`CheckpointError::VersionMismatch`] if the stored version differs from
    ///   [`METADATA_VERSION`],
    /// * [`CheckpointError::RepoPathMismatch`] if `repo_path` differs,
    /// * [`CheckpointError::AnalyzerMismatch`] if `analyzer_names` differs
    ///   (order-sensitive, like Go's `slices.Equal`).
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

/// Creates a directory and all parents with `0o750` permissions on Unix,
/// matching Go's `os.MkdirAll(dir, 0o750)`.
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

/// Returns the user's home directory, or `None` if it cannot be resolved.
///
/// Reproduces Go's `os.UserHomeDir` for the platforms this crate targets
/// without an extra dependency: `$HOME` on Unix, `%USERPROFILE%` on Windows.
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

/// Formats the current system time as an RFC3339 UTC string (`...Z`), matching
/// Go's `time.Now().UTC().Format(time.RFC3339)` at second precision.
///
/// Implemented without `chrono` (DESIGN §2.8 cautions against its formatter for
/// byte-identity). For deterministic output prefer [`Manager::save`] with an
/// injected timestamp; this helper backs the production default only.
fn now_rfc3339_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_unix_rfc3339(now.as_secs() as i64)
}

/// Converts a Unix timestamp (seconds) to an RFC3339 UTC string with `Z` suffix
/// and no fractional part, e.g. `2026-02-05T12:00:00Z`. This is the civil-time
/// conversion (proleptic Gregorian) Go's `time` package uses.
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
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
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
        items.iter().map(|s| s.to_string()).collect()
    }

    // Mirrors manager_test.go's `mockCheckpointable`.
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

    // Ported from TestManager_New.
    #[test]
    fn manager_new() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert_eq!(m.base_dir, dir.path());
        assert_eq!(m.repo_hash, "abc123");
        assert_eq!(m.max_age, DEFAULT_MAX_AGE);
        assert_eq!(m.max_size, DEFAULT_MAX_SIZE);
    }

    // Ported from TestManager_CheckpointDir.
    #[test]
    fn manager_checkpoint_dir() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert_eq!(m.checkpoint_dir(), dir.path().join("abc123"));
    }

    // Ported from TestManager_MetadataPath.
    #[test]
    fn manager_metadata_path() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert_eq!(
            m.metadata_path(),
            dir.path().join("abc123").join("checkpoint.json")
        );
    }

    // Ported from TestManager_Exists_NoCheckpoint.
    #[test]
    fn manager_exists_no_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert!(!m.exists());
    }

    // Ported from TestManager_Exists_WithCheckpoint.
    #[test]
    fn manager_exists_with_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        std::fs::create_dir_all(m.checkpoint_dir()).unwrap();
        std::fs::write(m.metadata_path(), br#"{"version":1}"#).unwrap();
        assert!(m.exists());
    }

    // Ported from TestManager_Clear.
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

    // Ported from TestManager_Clear_NonExistent.
    #[test]
    fn manager_clear_non_existent() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert!(m.clear().is_ok());
    }

    // Ported from TestManager_SaveLoad_Metadata.
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

    // Ported from TestManager_SaveLoad_Checkpointables.
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

    // Ported from TestManager_DefaultValues.
    #[test]
    fn manager_default_values() {
        assert_eq!(DEFAULT_MAX_AGE, Duration::from_secs(WEEK_SECONDS));
        assert_eq!(DEFAULT_MAX_SIZE, 1 << 30);
    }

    // Ported from TestManager_Validate_Success.
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

    // Ported from TestManager_Validate_WrongRepo.
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

    // Ported from TestManager_Validate_WrongAnalyzers.
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

    // Ported from TestManager_Validate_NoCheckpoint.
    #[test]
    fn manager_validate_no_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let m = Manager::new(dir.path(), "abc123");
        assert!(m.validate("/path/to/repo", &names(&["burndown"])).is_err());
    }

    // Ported from TestDefaultDir.
    #[test]
    fn default_dir_contains_codefang_checkpoints() {
        let dir = default_dir();
        let s = dir.to_string_lossy();
        assert!(s.contains(".codefang"));
        assert!(s.contains("checkpoints"));
    }

    // Ported from TestRepoHash.
    #[test]
    fn repo_hash_is_16_chars_and_stable() {
        let hash = repo_hash("/path/to/repo");
        assert_eq!(hash.len(), 16);
        assert_eq!(hash, repo_hash("/path/to/repo"));
        assert_ne!(hash, repo_hash("/different/repo"));
    }

    #[test]
    fn repo_hash_matches_go_sha256_first8_bytes() {
        // Independently compute the expected value the way Go does:
        // hex(sha256("/path/to/repo")[:8]).
        let full = Sha256::digest(b"/path/to/repo");
        let expected: String = full[..8].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(repo_hash("/path/to/repo"), expected);
    }

    // Ported from TestManager_Validate_OldVersion.
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

    // Ported from TestManager_SaveLoad_AggregatorSpills.
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

    // Ported from TestManager_Save_ErrorOnMkdir.
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

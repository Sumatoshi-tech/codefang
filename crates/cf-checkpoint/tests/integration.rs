//! Integration tests for crash/resume scenarios.
//!
//! These mirror the reference suite and exercise the public crate surface
//! (`Manager` + `Checkpointable`) the way an analyzer framework would: process
//! some commits, snapshot, simulate a crash with a fresh analyzer instance,
//! validate + restore, then resume.

use std::cell::RefCell;
use std::path::Path;

use cf_checkpoint::{repo_hash, CheckpointError, Checkpointable, Manager, Result, StreamingState};

const TEST_REPO_PATH: &str = "/test/repo";
const FIXED_TIME: &str = "2026-02-05T12:00:00Z";

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

/// Mirrors the reference suite's `mockAnalyzer`: persists its process log as
/// raw bytes (one byte per processed commit index).
struct MockAnalyzer {
    name: String,
    counter: RefCell<i64>,
    process_log: RefCell<Vec<i32>>,
}

impl MockAnalyzer {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            counter: RefCell::new(0),
            process_log: RefCell::new(Vec::new()),
        }
    }

    fn process(&self, commit_index: i32) {
        self.process_log.borrow_mut().push(commit_index);
        *self.counter.borrow_mut() += 1;
    }
}

impl Checkpointable for MockAnalyzer {
    fn save_checkpoint(&self, dir: &Path) -> Result<()> {
        let data: Vec<u8> = self.process_log.borrow().iter().map(|&v| v as u8).collect();
        std::fs::write(dir.join(format!("{}.bin", self.name)), data)?;
        Ok(())
    }

    fn load_checkpoint(&mut self, dir: &Path) -> Result<()> {
        let data = std::fs::read(dir.join(format!("{}.bin", self.name)))?;
        let log: Vec<i32> = data.iter().map(|&b| b as i32).collect();
        *self.counter.borrow_mut() = log.len() as i64;
        *self.process_log.borrow_mut() = log;
        Ok(())
    }

    fn checkpoint_size(&self) -> i64 {
        self.process_log.borrow().len() as i64
    }
}

// Mirrors TestCheckpoint_CrashAndResume.
#[test]
fn crash_and_resume() {
    let dir = tempfile::tempdir().unwrap();
    let repo_path = TEST_REPO_PATH;
    let hash = repo_hash(repo_path);

    // Process chunk 0 (commits 0-9) and chunk 1 (commits 10-19), then snapshot.
    let mut analyzer1 = MockAnalyzer::new("test");
    for i in 0..10 {
        analyzer1.process(i);
    }
    for i in 10..20 {
        analyzer1.process(i);
    }

    let mgr = Manager::new(dir.path(), hash);
    let state = StreamingState {
        total_commits: 30,
        processed_commits: 20,
        current_chunk: 1,
        total_chunks: 3,
        last_commit_hash: "abc123".into(),
        ..Default::default()
    };
    mgr.save(
        &mut [&mut analyzer1],
        state,
        repo_path,
        &names(&["test"]),
        FIXED_TIME.into(),
    )
    .unwrap();
    assert!(mgr.exists());

    // Simulate a crash, then restart with a fresh analyzer instance.
    let mut analyzer2 = MockAnalyzer::new("test");
    mgr.validate(repo_path, &names(&["test"])).unwrap();
    let loaded_state = mgr.load(&mut [&mut analyzer2]).unwrap();

    assert_eq!(analyzer2.process_log.borrow().len(), 20);
    assert_eq!(*analyzer2.counter.borrow(), 20);
    assert_eq!(loaded_state.current_chunk, 1);
    assert_eq!(loaded_state.processed_commits, 20);

    // Resume chunk 2 (commits 20-29).
    for i in 20..30 {
        analyzer2.process(i);
    }
    assert_eq!(analyzer2.process_log.borrow().len(), 30);
    for (i, &v) in analyzer2.process_log.borrow().iter().enumerate() {
        assert_eq!(v as usize, i, "commit {i} mismatch");
    }
}

// Mirrors TestCheckpoint_ResumeWithMismatchedRepo.
#[test]
fn resume_with_mismatched_repo() {
    let dir = tempfile::tempdir().unwrap();
    let hash = repo_hash(TEST_REPO_PATH);
    let mgr = Manager::new(dir.path(), hash);
    let state = StreamingState {
        total_commits: 100,
        ..Default::default()
    };
    mgr.save(
        &mut [],
        state,
        TEST_REPO_PATH,
        &names(&["burndown"]),
        FIXED_TIME.into(),
    )
    .unwrap();

    let err = mgr
        .validate("/different/repo", &names(&["burndown"]))
        .unwrap_err();
    assert!(matches!(err, CheckpointError::RepoPathMismatch { .. }));
}

// Mirrors TestCheckpoint_ResumeWithMismatchedAnalyzers.
#[test]
fn resume_with_mismatched_analyzers() {
    let dir = tempfile::tempdir().unwrap();
    let hash = repo_hash(TEST_REPO_PATH);
    let mgr = Manager::new(dir.path(), hash);
    let state = StreamingState {
        total_commits: 100,
        ..Default::default()
    };
    mgr.save(
        &mut [],
        state,
        TEST_REPO_PATH,
        &names(&["burndown"]),
        FIXED_TIME.into(),
    )
    .unwrap();

    let err = mgr.validate(TEST_REPO_PATH, &names(&["devs"])).unwrap_err();
    assert!(matches!(err, CheckpointError::AnalyzerMismatch { .. }));
}

// Mirrors TestCheckpoint_ClearAfterCompletion.
#[test]
fn clear_after_completion() {
    let dir = tempfile::tempdir().unwrap();
    let hash = repo_hash(TEST_REPO_PATH);
    let mgr = Manager::new(dir.path(), hash);
    let state = StreamingState {
        total_commits: 100,
        ..Default::default()
    };
    mgr.save(
        &mut [],
        state,
        TEST_REPO_PATH,
        &names(&["burndown"]),
        FIXED_TIME.into(),
    )
    .unwrap();
    assert!(mgr.exists());

    mgr.clear().unwrap();
    assert!(!mgr.exists());
}

// Mirrors TestCheckpoint_MultipleAnalyzers.
#[test]
fn multiple_analyzers() {
    let dir = tempfile::tempdir().unwrap();
    let hash = repo_hash(TEST_REPO_PATH);

    let mut analyzer1 = MockAnalyzer::new("burndown");
    let mut analyzer2 = MockAnalyzer::new("devs");
    for i in 0..5 {
        analyzer1.process(i);
        analyzer2.process(i * 10);
    }

    let mgr = Manager::new(dir.path(), hash);
    let state = StreamingState {
        total_commits: 10,
        processed_commits: 5,
        current_chunk: 0,
        total_chunks: 2,
        ..Default::default()
    };
    mgr.save(
        &mut [&mut analyzer1, &mut analyzer2],
        state,
        TEST_REPO_PATH,
        &names(&["burndown", "devs"]),
        FIXED_TIME.into(),
    )
    .unwrap();

    let mut restored1 = MockAnalyzer::new("burndown");
    let mut restored2 = MockAnalyzer::new("devs");
    mgr.load(&mut [&mut restored1, &mut restored2]).unwrap();

    assert_eq!(
        *analyzer1.process_log.borrow(),
        *restored1.process_log.borrow()
    );
    assert_eq!(
        *analyzer2.process_log.borrow(),
        *restored2.process_log.borrow()
    );
}

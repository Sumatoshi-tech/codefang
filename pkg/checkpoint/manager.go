package checkpoint

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// MetadataVersion is the current checkpoint metadata format version.
const MetadataVersion = 1

// Sentinel errors for checkpoint validation.
var (
	ErrRepoPathMismatch = errors.New("repo path mismatch")
	ErrAnalyzerMismatch = errors.New("analyzer mismatch")
)

// DefaultDir returns the default checkpoint directory (~/.codefang/checkpoints).
func DefaultDir() string {
	home, err := os.UserHomeDir()
	if err != nil {
		home = "."
	}

	return filepath.Join(home, ".codefang", "checkpoints")
}

// RepoHash computes a short hash of the repository path for use as directory name.
func RepoHash(repoPath string) string {
	h := sha256.Sum256([]byte(repoPath))

	return hex.EncodeToString(h[:8]) // First 8 bytes = 16 hex chars.
}

// Default retention values.
const (
	DefaultMaxAge  = 7 * 24 * time.Hour // 7 days.
	DefaultMaxSize = 1 << 30            // 1GB.
)

// Directory permissions for checkpoints.
const dirPerm = 0o750

// Manager coordinates checkpoints across analyzers.
type Manager struct {
	BaseDir  string
	RepoHash string
	MaxAge   time.Duration
	MaxSize  int64
}

// NewManager creates a new checkpoint manager.
func NewManager(baseDir, repoHash string) *Manager {
	return &Manager{
		BaseDir:  baseDir,
		RepoHash: repoHash,
		MaxAge:   DefaultMaxAge,
		MaxSize:  DefaultMaxSize,
	}
}

// CheckpointDir returns the directory for this repository's checkpoint.
func (m *Manager) CheckpointDir() string {
	return filepath.Join(m.BaseDir, m.RepoHash)
}

// MetadataPath returns the path to the metadata file.
func (m *Manager) MetadataPath() string {
	return filepath.Join(m.CheckpointDir(), "checkpoint.json")
}

// Exists returns true if a valid checkpoint exists.
func (m *Manager) Exists() bool {
	_, err := os.Stat(m.MetadataPath())

	return err == nil
}

// Clear removes the checkpoint for the current repository.
func (m *Manager) Clear() error {
	cpDir := m.CheckpointDir()

	_, statErr := os.Stat(cpDir)
	if os.IsNotExist(statErr) {
		return nil
	}

	err := os.RemoveAll(cpDir)
	if err != nil {
		return fmt.Errorf("remove checkpoint dir: %w", err)
	}

	return nil
}

// Save creates a checkpoint for all checkpointable analyzers.
func (m *Manager) Save(
	checkpointables []Checkpointable,
	state StreamingState,
	repoPath string,
	analyzerNames []string,
) error {
	cpDir := m.CheckpointDir()

	err := os.MkdirAll(cpDir, dirPerm)
	if err != nil {
		return fmt.Errorf("create checkpoint dir: %w", err)
	}

	checksums := make(map[string]string)

	// Save each checkpointable analyzer.
	for i, cp := range checkpointables {
		analyzerDir := filepath.Join(cpDir, fmt.Sprintf("analyzer_%d", i))

		mkdirErr := os.MkdirAll(analyzerDir, dirPerm)
		if mkdirErr != nil {
			return fmt.Errorf("create analyzer dir: %w", mkdirErr)
		}

		saveErr := cp.SaveCheckpoint(analyzerDir)
		if saveErr != nil {
			return fmt.Errorf("save checkpoint for analyzer %d: %w", i, saveErr)
		}
	}

	// Create metadata.
	meta := Metadata{
		Version:        MetadataVersion,
		RepoPath:       repoPath,
		RepoHash:       m.RepoHash,
		CreatedAt:      time.Now().UTC().Format(time.RFC3339),
		Analyzers:      analyzerNames,
		StreamingState: state,
		Checksums:      checksums,
	}

	// Write metadata.
	metaData, err := json.MarshalIndent(meta, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal metadata: %w", err)
	}

	writeErr := os.WriteFile(m.MetadataPath(), metaData, 0o600)
	if writeErr != nil {
		return fmt.Errorf("write metadata: %w", writeErr)
	}

	return nil
}

// LoadMetadata loads the checkpoint metadata.
func (m *Manager) LoadMetadata() (*Metadata, error) {
	data, err := os.ReadFile(m.MetadataPath())
	if err != nil {
		return nil, fmt.Errorf("read metadata: %w", err)
	}

	var meta Metadata

	unmarshalErr := json.Unmarshal(data, &meta)
	if unmarshalErr != nil {
		return nil, fmt.Errorf("unmarshal metadata: %w", unmarshalErr)
	}

	return &meta, nil
}

// Load restores state for all checkpointable analyzers.
func (m *Manager) Load(checkpointables []Checkpointable) (*StreamingState, error) {
	meta, err := m.LoadMetadata()
	if err != nil {
		return nil, err
	}

	cpDir := m.CheckpointDir()

	// Load each checkpointable analyzer.
	for i, cp := range checkpointables {
		analyzerDir := filepath.Join(cpDir, fmt.Sprintf("analyzer_%d", i))

		loadErr := cp.LoadCheckpoint(analyzerDir)
		if loadErr != nil {
			return nil, fmt.Errorf("load checkpoint for analyzer %d: %w", i, loadErr)
		}
	}

	return &meta.StreamingState, nil
}

// Validate checks if the checkpoint is valid for the given parameters.
func (m *Manager) Validate(repoPath string, analyzerNames []string) error {
	meta, err := m.LoadMetadata()
	if err != nil {
		return err
	}

	if meta.RepoPath != repoPath {
		return fmt.Errorf("%w: checkpoint has %q, got %q", ErrRepoPathMismatch, meta.RepoPath, repoPath)
	}

	if !stringSlicesEqual(meta.Analyzers, analyzerNames) {
		return fmt.Errorf("%w: checkpoint has %v, got %v", ErrAnalyzerMismatch, meta.Analyzers, analyzerNames)
	}

	return nil
}

// stringSlicesEqual compares two string slices for equality.
func stringSlicesEqual(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}

	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}

	return true
}

package analyze

import (
	"path/filepath"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/version"
)

// AnalysisMetadata holds provenance information for a codefang run.
type AnalysisMetadata struct {
	RepoPath        string `json:"repo_path"        yaml:"repo_path"`
	RepoName        string `json:"repo_name"        yaml:"repo_name"`
	AnalyzedAt      string `json:"analyzed_at"      yaml:"analyzed_at"`
	CodefangVersion string `json:"codefang_version" yaml:"codefang_version"`
}

// NewAnalysisMetadata creates metadata for the given repository path.
func NewAnalysisMetadata(repoPath string) *AnalysisMetadata {
	return &AnalysisMetadata{
		RepoPath:        repoPath,
		RepoName:        filepath.Base(repoPath),
		AnalyzedAt:      time.Now().UTC().Format(time.RFC3339),
		CodefangVersion: version.Version,
	}
}

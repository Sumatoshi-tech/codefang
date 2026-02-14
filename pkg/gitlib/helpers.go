package gitlib

import (
	"errors"
	"fmt"
	"log"
	"os"
	"regexp"
	"strings"
	"time"
)

// CommitLoadOptions configures how commits are loaded from a repository.
type CommitLoadOptions struct {
	Limit       int
	FirstParent bool
	HeadOnly    bool
	Since       string
}

// ErrInvalidTimeFormat is returned when a time string cannot be parsed.
var ErrInvalidTimeFormat = errors.New("cannot parse time")

// ErrRemoteNotSupported is returned when a remote repository URI is provided.
var ErrRemoteNotSupported = errors.New("remote repositories not supported")

// LoadRepository opens a local git repository. Returns an error for remote URIs.
func LoadRepository(uri string) (*Repository, error) {
	if strings.Contains(uri, "://") || regexp.MustCompile(`^[A-Za-z]\w*@[A-Za-z0-9][\w.]*:`).MatchString(uri) {
		return nil, fmt.Errorf("%w: %s", ErrRemoteNotSupported, uri)
	}

	if uri[len(uri)-1] == os.PathSeparator {
		uri = uri[:len(uri)-1]
	}

	repository, err := OpenRepository(uri)
	if err != nil {
		log.Fatalf("failed to open %s: %v", uri, err)
	}

	return repository, nil
}

// ParseTime parses a time string in various formats:
// - Duration relative to now (e.g. "24h")
// - RFC3339 (e.g. "2024-01-01T00:00:00Z")
// - Date only (e.g. "2024-01-01").
func ParseTime(s string) (time.Time, error) {
	d, durationErr := time.ParseDuration(s)
	if durationErr == nil {
		return time.Now().Add(-d), nil
	}

	parsedTime, rfc3339Err := time.Parse(time.RFC3339, s)
	if rfc3339Err == nil {
		return parsedTime, nil
	}

	parsedTime, dateOnlyErr := time.Parse(time.DateOnly, s)
	if dateOnlyErr == nil {
		return parsedTime, nil
	}

	return time.Time{}, fmt.Errorf("%w: %s", ErrInvalidTimeFormat, s)
}

// ReverseCommits reverses the order of commits (to oldest first).
func ReverseCommits(commits []*Commit) {
	for i, j := 0, len(commits)-1; i < j; i, j = i+1, j-1 {
		commits[i], commits[j] = commits[j], commits[i]
	}
}

// LoadCommits loads commits from a repository with the given options.
func LoadCommits(repository *Repository, opts CommitLoadOptions) ([]*Commit, error) {
	if opts.HeadOnly {
		return loadHeadCommit(repository)
	}

	return loadHistoryCommits(repository, opts)
}

func loadHeadCommit(repository *Repository) ([]*Commit, error) {
	headHash, err := repository.Head()
	if err != nil {
		return nil, fmt.Errorf("failed to get HEAD: %w", err)
	}

	commit, err := repository.LookupCommit(headHash)
	if err != nil {
		return nil, fmt.Errorf("failed to get commit: %w", err)
	}

	return []*Commit{commit}, nil
}

func loadHistoryCommits(repository *Repository, opts CommitLoadOptions) ([]*Commit, error) {
	logOpts := &LogOptions{
		FirstParent: opts.FirstParent,
	}

	if opts.Since != "" {
		sinceTime, parseErr := ParseTime(opts.Since)
		if parseErr != nil {
			return nil, fmt.Errorf("invalid time format for --since: %w", parseErr)
		}

		logOpts.Since = &sinceTime
	}

	iter, err := repository.Log(logOpts)
	if err != nil {
		return nil, fmt.Errorf("failed to list commits: %w", err)
	}
	defer iter.Close()

	commits := collectCommits(iter, opts.Limit)
	ReverseCommits(commits)

	return commits, nil
}

func collectCommits(iter *CommitIter, limit int) []*Commit {
	var commits []*Commit

	count := 0

	for {
		commit, err := iter.Next()
		if err != nil {
			break
		}

		if limit > 0 && count >= limit {
			commit.Free()

			break
		}

		commits = append(commits, commit)
		count++
	}

	return commits
}

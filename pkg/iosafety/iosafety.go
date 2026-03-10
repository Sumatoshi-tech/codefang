// Package iosafety provides defensive file-reading and terminal-output
// utilities for user-supplied paths and strings.
package iosafety

import (
	"errors"
	"fmt"
	"html"
	"os"
	"path/filepath"
	"strings"
	"unicode"
)

// Sentinel errors for path validation.
var (
	// ErrDirectoryPath indicates a file operation was attempted on a directory.
	ErrDirectoryPath = errors.New("path points to a directory")
	// ErrEmptyPath indicates a path argument was empty.
	ErrEmptyPath = errors.New("path is empty")
	// ErrPathContainsNUL indicates the path contains a NUL byte.
	ErrPathContainsNUL = errors.New("path contains NUL byte")
)

// ReadFile resolves, validates, and reads a user-supplied file path.
// Returns content, the resolved absolute path, and any error.
func ReadFile(path string) (content []byte, resolvedPath string, err error) {
	resolvedPath, err = ResolvePath(path)
	if err != nil {
		return nil, "", fmt.Errorf("resolve path %q: %w", path, err)
	}

	content, err = os.ReadFile(resolvedPath)
	if err != nil {
		return nil, "", fmt.Errorf("read %s: %w", resolvedPath, err)
	}

	return content, resolvedPath, nil
}

// ResolvePath normalises and validates a user-supplied file path.
// Returns the absolute path after cleaning, resolving, and stat-checking.
// Returns an error for empty paths, NUL bytes, directories, or stat failures.
func ResolvePath(path string) (string, error) {
	if strings.TrimSpace(path) == "" {
		return "", ErrEmptyPath
	}

	if strings.ContainsRune(path, '\x00') {
		return "", fmt.Errorf("%w: %q", ErrPathContainsNUL, path)
	}

	cleanPath := filepath.Clean(path)

	absPath, err := filepath.Abs(cleanPath)
	if err != nil {
		return "", fmt.Errorf("resolve absolute path for %q: %w", path, err)
	}

	info, err := os.Stat(absPath)
	if err != nil {
		return "", fmt.Errorf("stat %s: %w", absPath, err)
	}

	if info.IsDir() {
		return "", fmt.Errorf("%w: %s", ErrDirectoryPath, absPath)
	}

	return absPath, nil
}

// SanitizeForTerminal strips control characters and HTML-escapes the input.
// Newlines, carriage returns, and tabs are replaced with spaces.
func SanitizeForTerminal(input string) string {
	escaped := html.EscapeString(input)

	return strings.Map(func(r rune) rune {
		switch {
		case r == '\n' || r == '\r' || r == '\t':
			return ' '
		case unicode.IsControl(r):
			return -1
		default:
			return r
		}
	}, escaped)
}

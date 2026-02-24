package main

import (
	"errors"
	"fmt"
	"html"
	"os"
	"path/filepath"
	"strings"
	"unicode"
)

var (
	// ErrDirectoryPath indicates a file operation was attempted on a directory.
	ErrDirectoryPath = errors.New("path points to a directory")
	// ErrEmptyPath indicates a path argument was empty.
	ErrEmptyPath = errors.New("path is empty")
	// ErrPathContainsNUL indicates the path contains a NUL byte.
	ErrPathContainsNUL = errors.New("path contains NUL byte")
)

func safeReadFile(path string) (content []byte, resolvedPath string, err error) {
	resolvedPath, err = resolveUserFilePath(path)
	if err != nil {
		return nil, "", fmt.Errorf("resolve path %q: %w", path, err)
	}

	//nolint:gosec // resolvedPath is normalized and existence/type checked in resolveUserFilePath.
	content, err = os.ReadFile(resolvedPath)
	if err != nil {
		return nil, "", fmt.Errorf("read %s: %w", resolvedPath, err)
	}

	return content, resolvedPath, nil
}

func resolveUserFilePath(path string) (string, error) {
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

	//nolint:gosec // absPath is normalized by filepath.Clean + filepath.Abs.
	info, err := os.Stat(absPath)
	if err != nil {
		return "", fmt.Errorf("stat %s: %w", absPath, err)
	}

	if info.IsDir() {
		return "", fmt.Errorf("%w: %s", ErrDirectoryPath, absPath)
	}

	return absPath, nil
}

func sanitizeForTerminal(input string) string {
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

func writeTerminalLine(args ...any) {
	//nolint:gosec // CLI terminal output; values are sanitized by call sites where needed.
	fmt.Fprintln(os.Stdout, args...)
}

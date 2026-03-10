package iosafety

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// FRD: specs/frds/FRD-20260310-iosafety-promote.md.

func TestResolvePath_EmptyPath(t *testing.T) {
	t.Parallel()

	_, err := ResolvePath("")
	assert.ErrorIs(t, err, ErrEmptyPath)
}

func TestResolvePath_WhitespaceOnlyPath(t *testing.T) {
	t.Parallel()

	_, err := ResolvePath("   ")
	assert.ErrorIs(t, err, ErrEmptyPath)
}

func TestResolvePath_NULByte(t *testing.T) {
	t.Parallel()

	_, err := ResolvePath("file\x00name")
	assert.ErrorIs(t, err, ErrPathContainsNUL)
}

func TestResolvePath_Directory(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	_, err := ResolvePath(dir)
	assert.ErrorIs(t, err, ErrDirectoryPath)
}

func TestResolvePath_NonexistentFile(t *testing.T) {
	t.Parallel()

	_, err := ResolvePath("/nonexistent/path/file.txt")
	assert.Error(t, err)
}

func TestResolvePath_ValidFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	path := filepath.Join(dir, "test.txt")

	err := os.WriteFile(path, []byte("hello"), 0o600)
	require.NoError(t, err)

	resolved, err := ResolvePath(path)
	require.NoError(t, err)
	assert.True(t, filepath.IsAbs(resolved))
}

func TestResolvePath_ReturnsCleanAbsolutePath(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	path := filepath.Join(dir, "test.txt")

	err := os.WriteFile(path, []byte("hello"), 0o600)
	require.NoError(t, err)

	// Pass a relative-looking path with ".." components.
	dirtyPath := filepath.Join(dir, "subdir", "..", "test.txt")

	resolved, err := ResolvePath(dirtyPath)
	require.NoError(t, err)
	assert.Equal(t, filepath.Clean(resolved), resolved)
}

func TestReadFile_ValidFile(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	path := filepath.Join(dir, "data.txt")

	expected := []byte("file content")

	err := os.WriteFile(path, expected, 0o600)
	require.NoError(t, err)

	content, resolvedPath, err := ReadFile(path)
	require.NoError(t, err)
	assert.Equal(t, expected, content)
	assert.True(t, filepath.IsAbs(resolvedPath))
}

func TestReadFile_EmptyPath(t *testing.T) {
	t.Parallel()

	_, _, err := ReadFile("")
	assert.ErrorIs(t, err, ErrEmptyPath)
}

func TestReadFile_NonexistentFile(t *testing.T) {
	t.Parallel()

	_, _, err := ReadFile("/no/such/file.txt")
	assert.Error(t, err)
}

func TestReadFile_Directory(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	_, _, err := ReadFile(dir)
	assert.ErrorIs(t, err, ErrDirectoryPath)
}

func TestSanitizeForTerminal_PlainText(t *testing.T) {
	t.Parallel()

	got := SanitizeForTerminal("hello world")
	assert.Equal(t, "hello world", got)
}

func TestSanitizeForTerminal_HTMLEscaping(t *testing.T) {
	t.Parallel()

	got := SanitizeForTerminal("<script>alert('xss')</script>")
	assert.Contains(t, got, "&lt;script&gt;")
	assert.NotContains(t, got, "<script>")
}

func TestSanitizeForTerminal_ControlCharacters(t *testing.T) {
	t.Parallel()

	got := SanitizeForTerminal("hello\x00world\x07bell")
	assert.NotContains(t, got, "\x00")
	assert.NotContains(t, got, "\x07")
	assert.Contains(t, got, "hello")
	assert.Contains(t, got, "world")
}

func TestSanitizeForTerminal_WhitespaceReplacement(t *testing.T) {
	t.Parallel()

	got := SanitizeForTerminal("line1\nline2\ttab\rcarriage")
	assert.NotContains(t, got, "\n")
	assert.NotContains(t, got, "\t")
	assert.NotContains(t, got, "\r")
	assert.Contains(t, got, "line1 line2 tab carriage")
}

func TestSanitizeForTerminal_EmptyString(t *testing.T) {
	t.Parallel()

	got := SanitizeForTerminal("")
	assert.Empty(t, got)
}

package uast

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

const testGoSource = "package main\n\nfunc main() {}\n"

func TestParser_ParseFile_Success(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	file := filepath.Join(dir, "hello.go")
	writeTestGoFile(t, file)

	p, err := NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}

	root, err := p.ParseFile(context.Background(), file, "")
	if err != nil {
		t.Fatalf("ParseFile: %v", err)
	}

	if root == nil {
		t.Fatal("expected non-nil root node")
	}
}

func TestParser_ParseFile_LangOverride(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	// Write Go source with a .txt extension — needs lang override to parse as Go.
	file := filepath.Join(dir, "code.txt")
	writeTestGoFile(t, file)

	p, err := NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}

	root, err := p.ParseFile(context.Background(), file, "go")
	if err != nil {
		t.Fatalf("ParseFile with lang override: %v", err)
	}

	if root == nil {
		t.Fatal("expected non-nil root node with lang override")
	}
}

func TestParser_ParseFile_EmptyLang_AutoDetect(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	file := filepath.Join(dir, "auto.go")
	writeTestGoFile(t, file)

	p, err := NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}

	root, err := p.ParseFile(context.Background(), file, "")
	if err != nil {
		t.Fatalf("ParseFile auto-detect: %v", err)
	}

	if root == nil {
		t.Fatal("expected non-nil root node")
	}
}

func TestParser_ParseFile_FileNotFound(t *testing.T) {
	t.Parallel()

	p, err := NewParser()
	if err != nil {
		t.Fatalf("NewParser: %v", err)
	}

	_, err = p.ParseFile(context.Background(), "/nonexistent/path/foo.go", "")
	if err == nil {
		t.Fatal("expected error for nonexistent file")
	}
}

func writeTestGoFile(t *testing.T, path string) {
	t.Helper()

	err := os.WriteFile(path, []byte(testGoSource), 0o600)
	if err != nil {
		t.Fatalf("writeTestGoFile %s: %v", path, err)
	}
}

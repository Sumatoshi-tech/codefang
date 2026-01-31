package gitlib_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestDiffOpType(t *testing.T) {
	// Test that DiffOpType constants have expected values
	if gitlib.DiffOpEqual != 0 {
		t.Errorf("DiffOpEqual = %d, want 0", gitlib.DiffOpEqual)
	}

	if gitlib.DiffOpInsert != 1 {
		t.Errorf("DiffOpInsert = %d, want 1", gitlib.DiffOpInsert)
	}

	if gitlib.DiffOpDelete != 2 {
		t.Errorf("DiffOpDelete = %d, want 2", gitlib.DiffOpDelete)
	}
}

func TestBlobResultError(t *testing.T) {
	// Test error types
	tests := []struct {
		err      error
		expected string
	}{
		{gitlib.ErrRepositoryPointer, "failed to get repository pointer"},
		{gitlib.ErrBlobLookup, "blob lookup failed"},
		{gitlib.ErrBlobMemory, "memory allocation failed for blob"},
		{gitlib.ErrBlobBinary, "blob is binary"},
		{gitlib.ErrDiffLookup, "diff blob lookup failed"},
		{gitlib.ErrDiffMemory, "memory allocation failed for diff"},
		{gitlib.ErrDiffBinary, "diff blob is binary"},
		{gitlib.ErrDiffCompute, "diff computation failed"},
	}

	for _, tt := range tests {
		if tt.err.Error() != tt.expected {
			t.Errorf("Error() = %q, want %q", tt.err.Error(), tt.expected)
		}
	}
}

package gitlib

import (
	"testing"
)

func TestDiffOpType(t *testing.T) {
	// Test that DiffOpType constants have expected values
	if DiffOpEqual != 0 {
		t.Errorf("DiffOpEqual = %d, want 0", DiffOpEqual)
	}
	if DiffOpInsert != 1 {
		t.Errorf("DiffOpInsert = %d, want 1", DiffOpInsert)
	}
	if DiffOpDelete != 2 {
		t.Errorf("DiffOpDelete = %d, want 2", DiffOpDelete)
	}
}

func TestBlobResultError(t *testing.T) {
	// Test error types
	tests := []struct {
		err      error
		expected string
	}{
		{ErrRepositoryPointer, "failed to get repository pointer"},
		{ErrBlobLookup, "blob lookup failed"},
		{ErrBlobMemory, "memory allocation failed for blob"},
		{ErrBlobBinary, "blob is binary"},
		{ErrDiffLookup, "diff blob lookup failed"},
		{ErrDiffMemory, "memory allocation failed for diff"},
		{ErrDiffBinary, "diff blob is binary"},
		{ErrDiffCompute, "diff computation failed"},
	}

	for _, tt := range tests {
		if tt.err.Error() != tt.expected {
			t.Errorf("Error() = %q, want %q", tt.err.Error(), tt.expected)
		}
	}
}

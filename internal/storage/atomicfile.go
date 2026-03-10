// Package storage provides filesystem utilities for safe, atomic persistence.
package storage

import (
	"fmt"
	"io"
	"os"
)

const tmpSuffix = ".tmp"

// WriteAtomic writes to path atomically: creates a .tmp sibling, calls write
// with the temporary file, syncs the file to disk, then renames over path.
// If write returns an error or any step fails, the .tmp file is removed.
func WriteAtomic(path string, perm os.FileMode, write func(w io.Writer) error) error {
	tmpPath := path + tmpSuffix

	fd, createErr := os.OpenFile(tmpPath, os.O_WRONLY|os.O_CREATE|os.O_TRUNC, perm)
	if createErr != nil {
		return fmt.Errorf("atomic create %s: %w", tmpPath, createErr)
	}

	writeErr := write(fd)
	if writeErr != nil {
		fd.Close()
		os.Remove(tmpPath)

		return fmt.Errorf("atomic write %s: %w", path, writeErr)
	}

	syncErr := fd.Sync()
	if syncErr != nil {
		fd.Close()
		os.Remove(tmpPath)

		return fmt.Errorf("atomic sync %s: %w", path, syncErr)
	}

	closeErr := fd.Close()
	if closeErr != nil {
		os.Remove(tmpPath)

		return fmt.Errorf("atomic close %s: %w", path, closeErr)
	}

	renameErr := os.Rename(tmpPath, path)
	if renameErr != nil {
		os.Remove(tmpPath)

		return fmt.Errorf("atomic rename %s: %w", path, renameErr)
	}

	return nil
}

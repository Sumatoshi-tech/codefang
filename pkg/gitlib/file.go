package gitlib

import "io"

// FileIter iterates over files in a tree.
type FileIter struct {
	files []*File
	idx   int
}

// Next returns the next file in the iterator.
func (fi *FileIter) Next() (*File, error) {
	if fi.idx >= len(fi.files) {
		return nil, io.EOF
	}

	f := fi.files[fi.idx]
	fi.idx++

	return f, nil
}

// ForEach calls the callback for each file.
func (fi *FileIter) ForEach(cb func(*File) error) error {
	for _, file := range fi.files {
		cbErr := cb(file)
		if cbErr != nil {
			return cbErr
		}
	}

	return nil
}

// Close is a no-op for compatibility.
func (fi *FileIter) Close() {
	// No-op, but explicitly set idx to len(files) to indicate closed.
	fi.idx = len(fi.files)
}

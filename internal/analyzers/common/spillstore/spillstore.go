// Package spillstore provides generic disk-backed stores that spill accumulated
// data to temporary files during streaming hibernation, freeing memory between
// chunks while preserving the full dataset for Finalize.
package spillstore

import (
	"encoding/gob"
	"fmt"
	"maps"
	"os"
	"path/filepath"
)

// SpillStore wraps a map[string]V with transparent disk spilling.
//
// During normal (non-streaming) execution, it behaves as a plain map.
// When Spill() is called (typically from Hibernate()), the current buffer
// is written to a numbered gob file and the in-memory map is cleared.
// Collect() merges all spilled files and the current buffer into one map.
type SpillStore[V any] struct {
	current map[string]V
	dir     string // temp directory; created lazily on first Spill.
	spillN  int    // number of spill files written.
}

// New creates a SpillStore with an empty in-memory buffer.
func New[V any]() *SpillStore[V] {
	return &SpillStore[V]{
		current: make(map[string]V),
	}
}

// Put stores a key-value pair in the current in-memory buffer.
func (s *SpillStore[V]) Put(key string, val V) {
	s.current[key] = val
}

// Get returns a value from the current in-memory buffer.
// It does NOT read from spilled files.
func (s *SpillStore[V]) Get(key string) (V, bool) {
	v, ok := s.current[key]

	return v, ok
}

// Len returns the number of entries in the current in-memory buffer.
// Safe to call on a nil receiver (returns 0).
func (s *SpillStore[V]) Len() int {
	if s == nil {
		return 0
	}

	return len(s.current)
}

// Current returns the current in-memory buffer. The caller must not modify it.
func (s *SpillStore[V]) Current() map[string]V {
	return s.current
}

// Spill writes the current buffer to a numbered gob file and clears the map.
// No-op if the buffer is empty or the store is nil. The temp directory is created lazily.
func (s *SpillStore[V]) Spill() error {
	if s == nil || len(s.current) == 0 {
		return nil
	}

	if s.dir == "" {
		dir, err := os.MkdirTemp("", "codefang-spill-*")
		if err != nil {
			return fmt.Errorf("spillstore: create temp dir: %w", err)
		}

		s.dir = dir
	}

	path := filepath.Join(s.dir, fmt.Sprintf("chunk_%03d.gob", s.spillN))

	f, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("spillstore: create spill file: %w", err)
	}

	err = gob.NewEncoder(f).Encode(s.current)

	closeErr := f.Close()

	if err != nil {
		return fmt.Errorf("spillstore: encode spill %d: %w", s.spillN, err)
	}

	if closeErr != nil {
		return fmt.Errorf("spillstore: close spill %d: %w", s.spillN, closeErr)
	}

	s.spillN++
	s.current = make(map[string]V)

	return nil
}

// Collect returns all data (spilled + in-memory) merged into one map.
// Later entries overwrite earlier ones for the same key.
// After this call, spill files are cleaned up.
func (s *SpillStore[V]) Collect() (map[string]V, error) {
	return s.CollectWith(nil)
}

// CollectWith merges spilled chunks using an optional merge function.
// When merge is nil, later values overwrite earlier ones for duplicate keys.
// When merge is provided, it is called as merge(existing, incoming) for conflicts.
func (s *SpillStore[V]) CollectWith(merge func(existing, incoming V) V) (map[string]V, error) {
	result := make(map[string]V)

	for i := range s.spillN {
		chunk, err := s.readSpillFile(i)
		if err != nil {
			return nil, err
		}

		mergeInto(result, chunk, merge)
	}

	mergeInto(result, s.current, merge)

	s.Cleanup()
	s.current = make(map[string]V)
	s.spillN = 0

	return result, nil
}

// SpillCount returns the number of spill files written.
// Safe to call on a nil receiver (returns 0).
func (s *SpillStore[V]) SpillCount() int {
	if s == nil {
		return 0
	}

	return s.spillN
}

// SpillDir returns the temp directory path, or empty if no spills occurred.
// Safe to call on a nil receiver (returns "").
func (s *SpillStore[V]) SpillDir() string {
	if s == nil {
		return ""
	}

	return s.dir
}

// RestoreFromDir points the store at an existing spill directory with the
// given number of spill files. Used for checkpoint restoration.
func (s *SpillStore[V]) RestoreFromDir(dir string, count int) {
	s.dir = dir
	s.spillN = count

	if s.current == nil {
		s.current = make(map[string]V)
	}
}

// Cleanup removes the temp directory. Safe to call multiple times.
func (s *SpillStore[V]) Cleanup() {
	if s.dir != "" {
		os.RemoveAll(s.dir)
		s.dir = ""
	}
}

func (s *SpillStore[V]) readSpillFile(index int) (map[string]V, error) {
	path := filepath.Join(s.dir, fmt.Sprintf("chunk_%03d.gob", index))

	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("spillstore: open spill %d: %w", index, err)
	}

	defer f.Close()

	var chunk map[string]V

	err = gob.NewDecoder(f).Decode(&chunk)
	if err != nil {
		return nil, fmt.Errorf("spillstore: decode spill %d: %w", index, err)
	}

	return chunk, nil
}

func mergeInto[V any](dst, src map[string]V, merge func(V, V) V) {
	if merge == nil {
		maps.Copy(dst, src)

		return
	}

	for k, v := range src {
		if existing, ok := dst[k]; ok {
			dst[k] = merge(existing, v)
		} else {
			dst[k] = v
		}
	}
}

// Package burndown: PathInterner maps path strings to stable PathIDs for slice-backed state (Track B).

package burndown

import (
	"sync"
)

// PathID is a stable numeric id for an interned path. Used to index slice-backed state
// instead of map[string] so iteration is over a slice of active IDs, not map iteration.
type PathID uint32

// PathInterner maps path strings to stable PathIDs. Thread-safe. IDs are assigned
// sequentially (0, 1, 2, ...) so slice-backed state can use PathID as index.
type PathInterner struct {
	mu  sync.Mutex
	ids map[string]PathID
	rev []string
}

// NewPathInterner creates an empty PathInterner.
func NewPathInterner() *PathInterner {
	return &PathInterner{
		ids: make(map[string]PathID),
		rev: nil,
	}
}

// Intern returns the PathID for path, creating a new ID if path has not been seen.
// Safe for concurrent use.
func (pi *PathInterner) Intern(path string) PathID {
	pi.mu.Lock()
	defer pi.mu.Unlock()
	if id, ok := pi.ids[path]; ok {
		return id
	}
	id := PathID(len(pi.rev))
	pi.rev = append(pi.rev, path)
	pi.ids[path] = id
	return id
}

// Lookup returns the path string for id. Panics if id >= Len().
func (pi *PathInterner) Lookup(id PathID) string {
	pi.mu.Lock()
	defer pi.mu.Unlock()
	if int(id) >= len(pi.rev) {
		panic("PathID out of range")
	}
	return pi.rev[id]
}

// Len returns the number of interned paths (next Intern will return PathID(Len())).
func (pi *PathInterner) Len() int {
	pi.mu.Lock()
	defer pi.mu.Unlock()
	return len(pi.rev)
}

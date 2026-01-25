package toposort

import "sync"

// SymbolTable provides bidirectional mapping between strings and integer IDs.
type SymbolTable struct {
	strToID map[string]int
	idToStr []string
	lock    sync.RWMutex
}

// NewSymbolTable creates a new SymbolTable.
func NewSymbolTable() *SymbolTable {
	return &SymbolTable{
		strToID: make(map[string]int),
		idToStr: make([]string, 0),
	}
}

// Intern returns the unique ID for the given string.
// If the string is already interned, it returns the existing ID.
// Otherwise, it assigns a new ID and returns it.
func (s *SymbolTable) Intern(name string) int {
	s.lock.RLock()
	id, exists := s.strToID[name]
	s.lock.RUnlock()

	if exists {
		return id
	}

	s.lock.Lock()
	defer s.lock.Unlock()

	// Double check
	if id, exists := s.strToID[name]; exists {
		return id
	}

	id = len(s.idToStr)
	s.idToStr = append(s.idToStr, name)
	s.strToID[name] = id
	return id
}

// Resolve returns the string associated with the given ID.
// Returns an empty string if the ID is invalid.
func (s *SymbolTable) Resolve(id int) string {
	s.lock.RLock()
	defer s.lock.RUnlock()

	if id < 0 || id >= len(s.idToStr) {
		return ""
	}
	return s.idToStr[id]
}

// Len returns the number of symbols in the table.
func (s *SymbolTable) Len() int {
	s.lock.RLock()
	defer s.lock.RUnlock()
	return len(s.idToStr)
}

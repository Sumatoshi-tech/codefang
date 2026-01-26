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
		lock:    sync.RWMutex{},
	}
}

// Intern returns the unique ID for the given string.
// If the string is already interned, it returns the existing ID.
// Otherwise, it assigns a new ID and returns it.
func (table *SymbolTable) Intern(name string) int {
	table.lock.RLock()
	symbolID, exists := table.strToID[name]
	table.lock.RUnlock()

	if exists {
		return symbolID
	}

	table.lock.Lock()
	defer table.lock.Unlock()

	// Double check.
	if existingID, found := table.strToID[name]; found {
		return existingID
	}

	symbolID = len(table.idToStr)
	table.idToStr = append(table.idToStr, name)
	table.strToID[name] = symbolID

	return symbolID
}

// Resolve returns the string associated with the given ID.
// Returns an empty string if the ID is invalid.
func (table *SymbolTable) Resolve(id int) string {
	table.lock.RLock()
	defer table.lock.RUnlock()

	if id < 0 || id >= len(table.idToStr) {
		return ""
	}

	return table.idToStr[id]
}

// Len returns the number of symbols in the table.
func (table *SymbolTable) Len() int {
	table.lock.RLock()
	defer table.lock.RUnlock()

	return len(table.idToStr)
}

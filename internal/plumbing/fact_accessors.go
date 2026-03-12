package plumbing

import (
	"time"

	"github.com/Sumatoshi-tech/codefang/internal/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// GetTickSize extracts the tick duration from the facts map.
func GetTickSize(facts map[string]any) (time.Duration, bool) {
	val, ok := facts[FactTickSize].(time.Duration)

	return val, ok
}

// GetCommitsByTick extracts the commits-by-tick mapping from the facts map.
func GetCommitsByTick(facts map[string]any) (map[int][]gitlib.Hash, bool) {
	val, ok := facts[FactCommitsByTick].(map[int][]gitlib.Hash)

	return val, ok
}

// GetReversedPeopleDict extracts the reversed people dictionary from the facts map.
func GetReversedPeopleDict(facts map[string]any) ([]string, bool) {
	val, ok := facts[identity.FactIdentityDetectorReversedPeopleDict].([]string)

	return val, ok
}

// GetPeopleCount extracts the unique author count from the facts map.
func GetPeopleCount(facts map[string]any) (int, bool) {
	val, ok := facts[identity.FactIdentityDetectorPeopleCount].(int)

	return val, ok
}

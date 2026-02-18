package analyze

import (
	"errors"
	"fmt"
	pathpkg "path"
	"strings"
)

// AnalyzerMode identifies analyzer runtime mode.
type AnalyzerMode string

// Analyzer modes.
const (
	ModeStatic  AnalyzerMode = "static"
	ModeHistory AnalyzerMode = "history"
)

// ErrUnknownAnalyzerID is returned when registry lookup fails.
var ErrUnknownAnalyzerID = errors.New("unknown analyzer id")

// Descriptor contains stable analyzer metadata.
type Descriptor struct {
	ID          string
	Description string
	Mode        AnalyzerMode
}

// Registry stores analyzer metadata with deterministic ordering.
type Registry struct {
	ordered []Descriptor
	index   map[string]Descriptor
}

// ErrDuplicateAnalyzerID is returned when registry receives duplicate IDs.
var ErrDuplicateAnalyzerID = errors.New("duplicate analyzer id")

// ErrInvalidAnalyzerMode is returned when analyzer mode mismatches runtime category.
var ErrInvalidAnalyzerMode = errors.New("invalid analyzer mode")

// ErrInvalidAnalyzerGlob is returned when a glob pattern is malformed.
var ErrInvalidAnalyzerGlob = errors.New("invalid analyzer glob")

// NewRegistry creates a registry from analyzer descriptors.
func NewRegistry(static []StaticAnalyzer, history []HistoryAnalyzer) (*Registry, error) {
	ordered := make([]Descriptor, 0, len(static)+len(history))
	index := make(map[string]Descriptor, len(static)+len(history))

	err := appendDescriptors(ModeStatic, static, index, &ordered)
	if err != nil {
		return nil, err
	}

	err = appendDescriptors(ModeHistory, history, index, &ordered)
	if err != nil {
		return nil, err
	}

	return &Registry{
		ordered: ordered,
		index:   index,
	}, nil
}

func appendDescriptors[T Analyzer](
	mode AnalyzerMode,
	analyzers []T,
	index map[string]Descriptor,
	ordered *[]Descriptor,
) error {
	for _, analyzer := range analyzers {
		descriptor := analyzer.Descriptor()
		if descriptor.Mode == "" {
			descriptor.Mode = mode
		}

		if descriptor.Mode != mode {
			return fmt.Errorf("%w for %s: expected %s, got %s", ErrInvalidAnalyzerMode, descriptor.ID, mode, descriptor.Mode)
		}

		if _, exists := index[descriptor.ID]; exists {
			return fmt.Errorf("%w: %s", ErrDuplicateAnalyzerID, descriptor.ID)
		}

		index[descriptor.ID] = descriptor
		*ordered = append(*ordered, descriptor)
	}

	return nil
}

// All returns all descriptors in stable order.
func (r *Registry) All() []Descriptor {
	descriptors := make([]Descriptor, len(r.ordered))
	copy(descriptors, r.ordered)

	return descriptors
}

// IDsByMode returns IDs for the given mode in stable order.
func (r *Registry) IDsByMode(mode AnalyzerMode) []string {
	ids := make([]string, 0, len(r.ordered))

	for _, descriptor := range r.ordered {
		if descriptor.Mode == mode {
			ids = append(ids, descriptor.ID)
		}
	}

	return ids
}

// Descriptor returns analyzer metadata for the given ID.
func (r *Registry) Descriptor(id string) (Descriptor, bool) {
	descriptor, ok := r.index[id]

	return descriptor, ok
}

// Split divides analyzer IDs by mode while preserving provided order.
func (r *Registry) Split(ids []string) (staticIDs, historyIDs []string, err error) {
	staticIDs = make([]string, 0, len(ids))
	historyIDs = make([]string, 0, len(ids))

	for _, id := range ids {
		descriptor, ok := r.Descriptor(id)
		if !ok {
			return nil, nil, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, id)
		}

		if descriptor.Mode == ModeStatic {
			staticIDs = append(staticIDs, id)

			continue
		}

		historyIDs = append(historyIDs, id)
	}

	return staticIDs, historyIDs, nil
}

// ExpandPatterns expands glob patterns against registered analyzer IDs.
func (r *Registry) ExpandPatterns(patterns []string) ([]string, error) {
	idSet := r.descriptorIDSet()
	selected := make([]string, 0, len(r.ordered))
	selectedSet := make(map[string]struct{}, len(r.ordered))

	for _, rawPattern := range patterns {
		patternValue := strings.TrimSpace(rawPattern)

		ids, err := r.resolvePattern(patternValue, idSet)
		if err != nil {
			return nil, err
		}

		appendUniqueIDs(&selected, selectedSet, ids)
	}

	return selected, nil
}

// SelectedIDs returns the analyzer IDs for the given patterns, or all IDs if none specified.
func (r *Registry) SelectedIDs(patterns []string) ([]string, error) {
	if len(patterns) == 0 {
		return r.allIDs(), nil
	}

	return r.ExpandPatterns(patterns)
}

func (r *Registry) descriptorIDSet() map[string]struct{} {
	idSet := make(map[string]struct{}, len(r.ordered))
	for _, descriptor := range r.ordered {
		idSet[descriptor.ID] = struct{}{}
	}

	return idSet
}

func (r *Registry) resolvePattern(pattern string, idSet map[string]struct{}) ([]string, error) {
	if pattern == "" {
		return nil, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, pattern)
	}

	if !hasGlobMeta(pattern) {
		if _, exists := idSet[pattern]; !exists {
			return nil, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, pattern)
		}

		return []string{pattern}, nil
	}

	if pattern == "*" {
		return r.allIDs(), nil
	}

	matchedIDs, err := r.matchGlob(pattern)
	if err != nil {
		return nil, err
	}

	if len(matchedIDs) == 0 {
		return nil, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, pattern)
	}

	return matchedIDs, nil
}

func (r *Registry) matchGlob(pattern string) ([]string, error) {
	matched := make([]string, 0, len(r.ordered))
	for _, descriptor := range r.ordered {
		isMatch, err := pathpkg.Match(pattern, descriptor.ID)
		if err != nil {
			return nil, fmt.Errorf("%w %q: %w", ErrInvalidAnalyzerGlob, pattern, err)
		}

		if isMatch {
			matched = append(matched, descriptor.ID)
		}
	}

	return matched, nil
}

func (r *Registry) allIDs() []string {
	ids := make([]string, 0, len(r.ordered))
	for _, descriptor := range r.ordered {
		ids = append(ids, descriptor.ID)
	}

	return ids
}

func hasGlobMeta(pattern string) bool {
	return strings.ContainsAny(pattern, "*?[")
}

func appendUniqueIDs(target *[]string, targetSet map[string]struct{}, ids []string) {
	for _, id := range ids {
		if _, exists := targetSet[id]; exists {
			continue
		}

		*target = append(*target, id)
		targetSet[id] = struct{}{}
	}
}

// HistoryKeysByID maps history analyzer IDs to their pipeline keys.
func HistoryKeysByID(leaves map[string]HistoryAnalyzer, ids []string) ([]string, error) {
	idToKey := make(map[string]string, len(leaves))

	for key, analyzer := range leaves {
		idToKey[analyzer.Descriptor().ID] = key
	}

	keys := make([]string, 0, len(ids))
	for _, id := range ids {
		key, ok := idToKey[id]
		if !ok {
			return nil, fmt.Errorf("%w: %s", ErrUnknownAnalyzerID, id)
		}

		keys = append(keys, key)
	}

	return keys, nil
}

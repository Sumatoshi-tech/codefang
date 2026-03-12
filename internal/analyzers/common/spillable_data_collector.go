package common

import (
	"encoding/gob"
	"fmt"
	"maps"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

var registerGobTypes sync.Once

// defaultSpillThreshold is the default number of items before spilling to disk.
const defaultSpillThreshold = 10000

// estimatedItemBytes is the average gob-encoded size of a report item
// (map[string]any with ~8 keys). Used for memory estimation.
const estimatedItemBytes = 512

// compositeKeySeparator joins multiple identifier key values into a single dedup key.
const compositeKeySeparator = ":"

// SpillableDataCollector manages per-item data collection with transparent
// spill-to-disk when the in-memory buffer exceeds a configurable threshold.
// It collects per-item data keyed by identifier, with last-write-wins deduplication.
type SpillableDataCollector struct {
	buffer         map[string]map[string]any
	collectionKey  string
	identifierKey  string
	identifierKeys []string
	mode           analyze.AggregationMode
	spillDir       string
	spillN         int
	spillThreshold int
}

// NewSpillableDataCollector creates a collector that spills to disk when the
// in-memory item count reaches threshold. A threshold of 0 disables spilling.
func NewSpillableDataCollector(collectionKey, identifierKey string, threshold int) *SpillableDataCollector {
	registerGobTypes.Do(func() {
		gob.Register(map[string]any{})
		gob.Register([]map[string]any{})
		gob.Register([]any{})
	})

	return &SpillableDataCollector{
		buffer:         make(map[string]map[string]any),
		collectionKey:  collectionKey,
		identifierKey:  identifierKey,
		spillThreshold: threshold,
	}
}

// NewSpillableDataCollectorComposite creates a collector that uses multiple keys
// to build a composite dedup identifier. This prevents cross-file overwrites when
// items from different files share the same primary name.
// The last key in identifierKeys is used as the sort key for GetSortedData.
func NewSpillableDataCollectorComposite(collectionKey string, identifierKeys []string, threshold int) *SpillableDataCollector {
	registerGobTypes.Do(func() {
		gob.Register(map[string]any{})
		gob.Register([]map[string]any{})
		gob.Register([]any{})
	})

	// Use last key as fallback identifierKey for GetIdentifierKey() and sorting.
	var primaryKey string
	if len(identifierKeys) > 0 {
		primaryKey = identifierKeys[len(identifierKeys)-1]
	}

	return &SpillableDataCollector{
		buffer:         make(map[string]map[string]any),
		collectionKey:  collectionKey,
		identifierKey:  primaryKey,
		identifierKeys: identifierKeys,
		spillThreshold: threshold,
	}
}

// SetAggregationMode sets the aggregation mode.
// In [analyze.AggregationModeSummaryOnly], CollectFromReport becomes a no-op.
func (sdc *SpillableDataCollector) SetAggregationMode(mode analyze.AggregationMode) {
	sdc.mode = mode
}

// CollectFromReport extracts per-item data from a report.
// In [analyze.AggregationModeSummaryOnly] mode, this is a no-op.
// Handles both legacy []map[string]any and [analyze.TypedCollection] values.
func (sdc *SpillableDataCollector) CollectFromReport(report analyze.Report) {
	if sdc.mode == analyze.AggregationModeSummaryOnly {
		return
	}

	collection := sdc.extractCollection(report)
	if collection == nil {
		return
	}

	for _, item := range collection {
		identifier := sdc.extractIdentifier(item)
		if identifier == "" {
			continue
		}

		sdc.buffer[identifier] = item
	}

	sdc.spillIfNeeded()
}

// extractCollection extracts the collection slice from a report value.
// Supports [analyze.TypedCollection] (converts via ToMaps) and legacy []map[string]any.
func (sdc *SpillableDataCollector) extractCollection(report analyze.Report) []map[string]any {
	val := report[sdc.collectionKey]
	if val == nil {
		return nil
	}

	if tc, ok := val.(analyze.TypedCollection); ok {
		return tc.ToMaps(tc.Items, tc.SourceFile)
	}

	if collection, ok := val.([]map[string]any); ok {
		return collection
	}

	return nil
}

// GetSortedData returns all collected items (in-memory + spilled) sorted by
// identifier key. Spill files are cleaned up after this call.
func (sdc *SpillableDataCollector) GetSortedData() []map[string]any {
	merged := sdc.mergeAll()

	data := make([]map[string]any, 0, len(merged))
	for _, item := range merged {
		data = append(data, item)
	}

	sort.Slice(data, func(i, j int) bool {
		nameI := extractStringKey(data[i], sdc.identifierKey)
		nameJ := extractStringKey(data[j], sdc.identifierKey)

		return nameI < nameJ
	})

	sdc.Cleanup()
	sdc.buffer = make(map[string]map[string]any)
	sdc.spillN = 0

	return data
}

// GetDataCount returns the number of items in the current in-memory buffer.
// This does not include spilled items.
func (sdc *SpillableDataCollector) GetDataCount() int {
	return len(sdc.buffer)
}

// GetCollectionKey returns the collection key.
func (sdc *SpillableDataCollector) GetCollectionKey() string {
	return sdc.collectionKey
}

// GetIdentifierKey returns the identifier key.
func (sdc *SpillableDataCollector) GetIdentifierKey() string {
	return sdc.identifierKey
}

// SpillCount returns the number of spill files written.
func (sdc *SpillableDataCollector) SpillCount() int {
	return sdc.spillN
}

// SpillDir returns the temp directory path, or empty if no spills occurred.
func (sdc *SpillableDataCollector) SpillDir() string {
	return sdc.spillDir
}

// EstimatedBufferBytes returns the estimated memory usage of the in-memory buffer.
// Does not include spilled data on disk.
func (sdc *SpillableDataCollector) EstimatedBufferBytes() int64 {
	return int64(len(sdc.buffer)) * estimatedItemBytes
}

// Cleanup removes the temp spill directory. Safe to call multiple times.
func (sdc *SpillableDataCollector) Cleanup() {
	if sdc.spillDir != "" {
		os.RemoveAll(sdc.spillDir)
		sdc.spillDir = ""
	}
}

// spillIfNeeded spills the buffer to disk if it exceeds the threshold.
// On spill failure, the threshold is disabled to prevent repeated attempts.
func (sdc *SpillableDataCollector) spillIfNeeded() {
	if sdc.spillThreshold <= 0 || len(sdc.buffer) < sdc.spillThreshold {
		return
	}

	err := sdc.spill()
	if err != nil {
		sdc.spillThreshold = 0
	}
}

// spill writes the current buffer to a numbered gob file and clears the buffer.
func (sdc *SpillableDataCollector) spill() error {
	if len(sdc.buffer) == 0 {
		return nil
	}

	if sdc.spillDir == "" {
		dir, err := os.MkdirTemp("", "codefang-spill-dc-*")
		if err != nil {
			return fmt.Errorf("spillable data collector: create temp dir: %w", err)
		}

		sdc.spillDir = dir
	}

	path := filepath.Join(sdc.spillDir, fmt.Sprintf("chunk_%03d.gob", sdc.spillN))

	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		return fmt.Errorf("spillable data collector: create chunk %d: %w", sdc.spillN, err)
	}

	encErr := gob.NewEncoder(f).Encode(sdc.buffer)

	closeErr := f.Close()

	if encErr != nil {
		return fmt.Errorf("spillable data collector: encode chunk %d: %w", sdc.spillN, encErr)
	}

	if closeErr != nil {
		return fmt.Errorf("spillable data collector: close chunk %d: %w", sdc.spillN, closeErr)
	}

	sdc.spillN++
	sdc.buffer = make(map[string]map[string]any)

	return nil
}

// mergeAll reads all spill files and merges with the in-memory buffer.
// Later entries overwrite earlier ones for the same key (last-write-wins).
func (sdc *SpillableDataCollector) mergeAll() map[string]map[string]any {
	result := make(map[string]map[string]any)

	for i := range sdc.spillN {
		chunk, err := sdc.readSpillFile(i)
		if err != nil {
			continue
		}

		maps.Copy(result, chunk)
	}

	maps.Copy(result, sdc.buffer)

	return result
}

func (sdc *SpillableDataCollector) readSpillFile(index int) (map[string]map[string]any, error) {
	path := filepath.Join(sdc.spillDir, fmt.Sprintf("chunk_%03d.gob", index))

	f, err := os.Open(path)
	if err != nil {
		return nil, fmt.Errorf("spillable data collector: open chunk %d: %w", index, err)
	}
	defer f.Close()

	var chunk map[string]map[string]any

	err = gob.NewDecoder(f).Decode(&chunk)
	if err != nil {
		return nil, fmt.Errorf("spillable data collector: decode chunk %d: %w", index, err)
	}

	return chunk, nil
}

// extractIdentifier builds the dedup key from the item.
// For composite keys, it joins all key values with ":".
// For single key, it extracts the value directly.
// Returns empty string if any required key is missing.
func (sdc *SpillableDataCollector) extractIdentifier(item map[string]any) string {
	if len(sdc.identifierKeys) > 0 {
		return sdc.buildCompositeKey(item)
	}

	v, ok := item[sdc.identifierKey].(string)
	if !ok {
		return ""
	}

	return v
}

// buildCompositeKey joins values of identifierKeys with compositeKeySeparator.
// The last key (primary identifier) is required; earlier keys are optional.
// When an optional key is missing, it is omitted from the composite.
// This allows graceful fallback when _source_file is not yet stamped.
func (sdc *SpillableDataCollector) buildCompositeKey(item map[string]any) string {
	lastIdx := len(sdc.identifierKeys) - 1

	// Last key is required.
	lastVal, ok := item[sdc.identifierKeys[lastIdx]].(string)
	if !ok {
		return ""
	}

	if lastIdx == 0 {
		return lastVal
	}

	var b strings.Builder

	for _, k := range sdc.identifierKeys[:lastIdx] {
		if v, vOK := item[k].(string); vOK {
			b.WriteString(v)
			b.WriteString(compositeKeySeparator)
		}
	}

	b.WriteString(lastVal)

	return b.String()
}

// extractStringKey safely extracts a string value from a map.
func extractStringKey(m map[string]any, key string) string {
	v, ok := m[key].(string)
	if !ok {
		return ""
	}

	return v
}

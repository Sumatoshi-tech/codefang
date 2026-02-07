package devs

import (
	"github.com/Sumatoshi-tech/codefang/pkg/checkpoint"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// checkpointBasename is the base filename for checkpoint files (used by tests).
const checkpointBasename = "devs_state"

// Checkpoint size estimation constants.
const (
	baseOverheadBytes = 100
	bytesPerTick      = 150
	bytesPerMerge     = 44
	bytesPerPerson    = 50
)

// newPersister creates a checkpoint persister for devs analyzer.
func newPersister() *checkpoint.Persister[checkpointState] {
	return checkpoint.NewPersister[checkpointState](
		checkpointBasename,
		checkpoint.NewJSONCodec(),
	)
}

// SaveCheckpoint writes the analyzer state to the given directory.
func (d *HistoryAnalyzer) SaveCheckpoint(dir string) error {
	return newPersister().Save(dir, d.buildCheckpointState)
}

// LoadCheckpoint restores the analyzer state from the given directory.
func (d *HistoryAnalyzer) LoadCheckpoint(dir string) error {
	return newPersister().Load(dir, d.restoreFromCheckpoint)
}

// checkpointState holds the serializable state of the devs analyzer.
type checkpointState struct {
	Ticks  map[int]map[int]*serializableDevTick `json:"ticks"`
	Merges []string                             `json:"merges"`
}

// serializableDevTick mirrors DevTick with JSON-friendly structure.
type serializableDevTick struct {
	Commits   int                           `json:"commits"`
	Added     int                           `json:"added"`
	Removed   int                           `json:"removed"`
	Changed   int                           `json:"changed"`
	Languages map[string]*serializableStats `json:"languages,omitempty"`
}

// serializableStats holds line statistics for JSON serialization.
type serializableStats struct {
	Added   int `json:"added"`
	Removed int `json:"removed"`
	Changed int `json:"changed"`
}

// buildCheckpointState creates a serializable snapshot of the analyzer state.
func (d *HistoryAnalyzer) buildCheckpointState() *checkpointState {
	state := &checkpointState{
		Ticks:  make(map[int]map[int]*serializableDevTick),
		Merges: make([]string, 0, len(d.merges)),
	}

	for tick, devTicks := range d.ticks {
		state.Ticks[tick] = make(map[int]*serializableDevTick)

		for devID, devTick := range devTicks {
			state.Ticks[tick][devID] = convertDevTickToSerializable(devTick)
		}
	}

	for hash := range d.merges {
		state.Merges = append(state.Merges, hash.String())
	}

	return state
}

func convertDevTickToSerializable(dt *DevTick) *serializableDevTick {
	sdt := &serializableDevTick{
		Commits: dt.Commits,
		Added:   dt.Added,
		Removed: dt.Removed,
		Changed: dt.Changed,
	}

	if len(dt.Languages) > 0 {
		sdt.Languages = make(map[string]*serializableStats)

		for lang, stats := range dt.Languages {
			sdt.Languages[lang] = &serializableStats{
				Added:   stats.Added,
				Removed: stats.Removed,
				Changed: stats.Changed,
			}
		}
	}

	return sdt
}

// restoreFromCheckpoint restores analyzer state from a checkpoint.
func (d *HistoryAnalyzer) restoreFromCheckpoint(state *checkpointState) {
	d.ticks = make(map[int]map[int]*DevTick)

	for tick, devTicks := range state.Ticks {
		d.ticks[tick] = make(map[int]*DevTick)

		for devID, sdt := range devTicks {
			d.ticks[tick][devID] = convertSerializableToDevTick(sdt)
		}
	}

	d.merges = make(map[gitlib.Hash]bool, len(state.Merges))

	for _, hashStr := range state.Merges {
		d.merges[gitlib.NewHash(hashStr)] = true
	}
}

func convertSerializableToDevTick(sdt *serializableDevTick) *DevTick {
	dt := &DevTick{
		Commits:   sdt.Commits,
		Languages: make(map[string]pkgplumbing.LineStats),
	}
	dt.Added = sdt.Added
	dt.Removed = sdt.Removed
	dt.Changed = sdt.Changed

	for lang, stats := range sdt.Languages {
		dt.Languages[lang] = pkgplumbing.LineStats{
			Added:   stats.Added,
			Removed: stats.Removed,
			Changed: stats.Changed,
		}
	}

	return dt
}

// CheckpointSize returns an estimated size of the checkpoint in bytes.
func (d *HistoryAnalyzer) CheckpointSize() int64 {
	size := int64(baseOverheadBytes)

	// Count tick entries.
	for _, devTicks := range d.ticks {
		size += int64(len(devTicks) * bytesPerTick)
	}

	// Count merge entries.
	size += int64(len(d.merges) * bytesPerMerge)

	// Count people entries.
	size += int64(len(d.reversedPeopleDict) * bytesPerPerson)

	return size
}

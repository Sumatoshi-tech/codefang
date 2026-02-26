package clones

import (
	"fmt"
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
)

// Synergy threshold constants.
const (
	// synergyCouplingThreshold is the minimum coupling strength for a synergy signal.
	synergyCouplingThreshold = 0.3

	// synergySimilarityThreshold is the minimum clone similarity for a synergy signal.
	synergySimilarityThreshold = 0.8
)

// RefactoringSignal represents a cross-referenced signal from coupling and clone analysis.
// When files frequently change together AND contain similar functions, this is a
// strong indicator that shared code should be extracted.
type RefactoringSignal struct {
	FileA            string  `json:"file_a"            yaml:"file_a"`
	FileB            string  `json:"file_b"            yaml:"file_b"`
	CouplingStrength float64 `json:"coupling_strength" yaml:"coupling_strength"`
	CloneSimilarity  float64 `json:"clone_similarity"  yaml:"clone_similarity"`
	Recommendation   string  `json:"recommendation"    yaml:"recommendation"`
}

// ComputeSynergy cross-references couples coupling data with clone detection pairs.
// It returns refactoring signals for file pairs that exceed both the coupling threshold
// (> 0.3) and the clone similarity threshold (> 0.8).
func ComputeSynergy(couplingData []couples.FileCouplingData, clonePairs []ClonePair) []RefactoringSignal {
	if len(couplingData) == 0 || len(clonePairs) == 0 {
		return nil
	}

	cloneLookup := buildCloneLookup(clonePairs)

	var signals []RefactoringSignal

	for _, coupling := range couplingData {
		if coupling.Strength <= synergyCouplingThreshold {
			continue
		}

		signal, ok := matchCouplingWithClones(coupling, cloneLookup)
		if ok {
			signals = append(signals, signal)
		}
	}

	sortSignalsByStrength(signals)

	return signals
}

// cloneLookupEntry stores the maximum similarity for a file pair.
type cloneLookupEntry struct {
	maxSimilarity float64
}

// buildCloneLookup creates a map from canonical file pair key to max similarity.
func buildCloneLookup(clonePairs []ClonePair) map[string]cloneLookupEntry {
	lookup := make(map[string]cloneLookupEntry, len(clonePairs))

	for _, pair := range clonePairs {
		key := clonePairKey(pair.FuncA, pair.FuncB)
		entry := lookup[key]

		if pair.Similarity > entry.maxSimilarity {
			entry.maxSimilarity = pair.Similarity
		}

		lookup[key] = entry
	}

	return lookup
}

// matchCouplingWithClones checks if a coupled file pair has matching clone data.
func matchCouplingWithClones(coupling couples.FileCouplingData, cloneLookup map[string]cloneLookupEntry) (RefactoringSignal, bool) {
	key := clonePairKey(coupling.File1, coupling.File2)

	entry, found := cloneLookup[key]
	if !found || entry.maxSimilarity <= synergySimilarityThreshold {
		return RefactoringSignal{}, false
	}

	return RefactoringSignal{
		FileA:            coupling.File1,
		FileB:            coupling.File2,
		CouplingStrength: coupling.Strength,
		CloneSimilarity:  entry.maxSimilarity,
		Recommendation:   buildRecommendation(coupling.File1, coupling.File2),
	}, true
}

// buildRecommendation creates a human-readable recommendation for a refactoring signal.
func buildRecommendation(fileA, fileB string) string {
	return fmt.Sprintf("Files %s and %s are tightly coupled and contain similar code. Consider extracting shared logic.", fileA, fileB)
}

// sortSignalsByStrength sorts signals by combined strength (coupling * similarity) descending.
func sortSignalsByStrength(signals []RefactoringSignal) {
	sort.Slice(signals, func(i, j int) bool {
		strengthI := signals[i].CouplingStrength * signals[i].CloneSimilarity
		strengthJ := signals[j].CouplingStrength * signals[j].CloneSimilarity

		return strengthI > strengthJ
	})
}

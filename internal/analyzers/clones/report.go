package clones

import "github.com/Sumatoshi-tech/codefang/internal/analyzers/common"

// Clone type constants.
const (
	// CloneType1 represents an exact clone (identical AST structure and tokens).
	CloneType1 = "Type-1"

	// CloneType2 represents a renamed clone (identical AST structure, different tokens).
	CloneType2 = "Type-2"

	// CloneType3 represents a near-miss clone (similar AST structure).
	CloneType3 = "Type-3"
)

// Similarity thresholds for clone classification.
const (
	// similarityExact is the threshold for Type-1 (exact) clones.
	similarityExact = 1.0

	// similarityType2 is the minimum threshold for Type-2 (renamed) clones.
	similarityType2 = 0.8

	// similarityType3 is the minimum threshold for Type-3 (near-miss) clones.
	similarityType3 = 0.5
)

// Report keys.
const (
	keyAnalyzerName    = "analyzer_name"
	keyTotalClonePairs = "total_clone_pairs"
	keyClonePairs      = "clone_pairs"
	keyTotalFunctions  = "total_functions"
	keyMessage         = "message"
	keyCloneRatio      = "clone_ratio"
	keyFuncSignatures  = "_func_signatures"
)

// ClonePair represents a detected clone relationship between two functions.
type ClonePair struct {
	FuncA      string  `json:"func_a"     yaml:"func_a"`
	FuncB      string  `json:"func_b"     yaml:"func_b"`
	Similarity float64 `json:"similarity" yaml:"similarity"`
	CloneType  string  `json:"clone_type" yaml:"clone_type"`
}

// ComputedMetrics holds computed clone detection metrics for JSON/YAML/binary export.
type ComputedMetrics struct {
	TotalFunctions  int         `json:"total_functions"   yaml:"total_functions"`
	TotalClonePairs int         `json:"total_clone_pairs" yaml:"total_clone_pairs"`
	CloneRatio      float64     `json:"clone_ratio"       yaml:"clone_ratio"`
	ClonePairs      []ClonePair `json:"clone_pairs"       yaml:"clone_pairs"`
	Message         string      `json:"message"           yaml:"message"`
}

// cloneTypeClassifier classifies clone similarity into clone types.
var cloneTypeClassifier = common.NewClassifier([]common.Threshold[float64]{
	{Limit: similarityExact, Label: CloneType1},
	{Limit: similarityType2, Label: CloneType2},
}, CloneType3)

// classifyCloneType determines the clone type based on similarity score.
func classifyCloneType(similarity float64) string {
	return cloneTypeClassifier.Classify(similarity)
}

// PairKey is a canonical key for a clone pair to avoid duplicates.
type PairKey struct {
	FuncA string
	FuncB string
}

// clonePairKey returns a canonical key for a clone pair to avoid duplicates.
// The key is ordered alphabetically so (A,B) and (B,A) produce the same key.
func clonePairKey(funcA, funcB string) PairKey {
	if funcA > funcB {
		funcA, funcB = funcB, funcA
	}

	return PairKey{FuncA: funcA, FuncB: funcB}
}

// computeMetricsFromReport extracts ComputedMetrics from a report map.
func computeMetricsFromReport(report map[string]any) *ComputedMetrics {
	metrics := &ComputedMetrics{}

	if v, ok := report[keyTotalFunctions]; ok {
		if n, nOK := v.(int); nOK {
			metrics.TotalFunctions = n
		}
	}

	if v, ok := report[keyTotalClonePairs]; ok {
		if n, nOK := v.(int); nOK {
			metrics.TotalClonePairs = n
		}
	}

	if v, ok := report[keyCloneRatio]; ok {
		if f, fOK := v.(float64); fOK {
			metrics.CloneRatio = f
		}
	}

	if v, ok := report[keyMessage]; ok {
		if s, sOK := v.(string); sOK {
			metrics.Message = s
		}
	}

	metrics.ClonePairs = extractClonePairs(report)

	return metrics
}

// extractClonePairs extracts clone pairs from a report.
func extractClonePairs(report map[string]any) []ClonePair {
	raw, ok := report[keyClonePairs]
	if !ok {
		return nil
	}

	if pairs, typedOK := raw.([]ClonePair); typedOK {
		return pairs
	}

	// Direct []map[string]any — produced by the aggregator's GetResult().
	if maps, mapsOK := raw.([]map[string]any); mapsOK {
		return clonePairsFromMaps(maps)
	}

	// []any — produced by JSON decoding.
	return extractClonePairsFromAny(raw)
}

// clonePairsFromMaps converts a []map[string]any slice to []ClonePair.
func clonePairsFromMaps(maps []map[string]any) []ClonePair {
	pairs := make([]ClonePair, 0, len(maps))

	for _, m := range maps {
		pairs = append(pairs, clonePairFromMap(m))
	}

	return pairs
}

// extractClonePairsFromAny handles JSON-decoded clone pairs stored as []any.
func extractClonePairsFromAny(raw any) []ClonePair {
	items, ok := raw.([]any)
	if !ok {
		return nil
	}

	pairs := make([]ClonePair, 0, len(items))

	for _, item := range items {
		m, mOK := item.(map[string]any)
		if !mOK {
			continue
		}

		pair := clonePairFromMap(m)
		pairs = append(pairs, pair)
	}

	return pairs
}

// clonePairFromMap extracts a ClonePair from a map.
func clonePairFromMap(m map[string]any) ClonePair {
	pair := ClonePair{}

	if v, ok := m["func_a"]; ok {
		if s, sOK := v.(string); sOK {
			pair.FuncA = s
		}
	}

	if v, ok := m["func_b"]; ok {
		if s, sOK := v.(string); sOK {
			pair.FuncB = s
		}
	}

	if v, ok := m["similarity"]; ok {
		if f, fOK := v.(float64); fOK {
			pair.Similarity = f
		}
	}

	if v, ok := m["clone_type"]; ok {
		if s, sOK := v.(string); sOK {
			pair.CloneType = s
		}
	}

	return pair
}

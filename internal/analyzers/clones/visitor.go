package clones

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/lsh"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Visitor implements the AnalysisVisitor interface for clone detection.
// It collects function nodes during traversal and exports MinHash signatures
// for cross-file clone detection by the aggregator.
type Visitor struct {
	functions       []*node.Node
	shingler        *Shingler
	numHashes       int
	similarityType2 float64
	similarityType3 float64
}

// NewVisitor creates a new clone detection Visitor.
func NewVisitor() *Visitor {
	return &Visitor{
		shingler:        NewShingler(defaultShingleSize),
		numHashes:       numHashes,
		similarityType2: similarityType2,
		similarityType3: similarityType3,
	}
}

// OnEnter is called when entering a node during traversal.
func (v *Visitor) OnEnter(n *node.Node, _ int) {
	if isFunctionNode(n) {
		v.functions = append(v.functions, n)
	}
}

// OnExit is called when exiting a node during traversal.
func (v *Visitor) OnExit(_ *node.Node, _ int) {
	// No action needed on exit.
}

// GetReport returns the clone detection report with function signatures.
// Detection is deferred to the aggregator for cross-file comparison.
func (v *Visitor) GetReport() analyze.Report {
	if len(v.functions) == 0 {
		return buildEmptyReport(msgNoFunctions)
	}

	entries := v.buildSignatures()

	return buildSignatureReport(len(v.functions), entries)
}

// buildSignatures computes MinHash signatures for all collected functions.
func (v *Visitor) buildSignatures() []funcEntry {
	entries := make([]funcEntry, 0, len(v.functions))

	for _, fn := range v.functions {
		if countNodes(fn) < minFunctionNodes {
			continue
		}

		shingles := v.shingler.ExtractShingles(fn)
		if len(shingles) == 0 {
			continue
		}

		sig, err := minhash.New(v.numHashes)
		if err != nil {
			continue
		}

		for _, shingle := range shingles {
			sig.Add(shingle)
		}

		name := extractFuncName(fn)

		entries = append(entries, funcEntry{
			name: name,
			sig:  sig,
		})
	}

	return entries
}

// buildSignatureReport constructs a report that exports function signatures
// for the aggregator to perform cross-file clone detection.
func buildSignatureReport(totalFunctions int, entries []funcEntry) analyze.Report {
	sigEntries := make([]map[string]any, 0, len(entries))

	for _, e := range entries {
		sigEntries = append(sigEntries, map[string]any{
			"name": e.name,
			"sig":  e.sig,
		})
	}

	return analyze.Report{
		keyAnalyzerName:    analyzerName,
		keyTotalFunctions:  totalFunctions,
		keyTotalClonePairs: 0,
		keyCloneRatio:      0.0,
		keyClonePairs:      []map[string]any{},
		keyMessage:         msgNoClones,
		keyFuncSignatures:  sigEntries,
	}
}

// findClonePairs queries the LSH index and collects unique clone pairs.
// pairCap limits the stored pairs slice (0 = unlimited). The returned totalCount
// reflects ALL unique pairs found, regardless of the cap.
// clonePairResult holds the output of findClonePairs.
type clonePairResult struct {
	pairs            []ClonePair
	totalCount       int
	typeDistribution cloneTypeCounts
	clonedFunc       map[string]struct{} // distinct function names involved in any pair.
}

func findClonePairs(entries []funcEntry, idx *lsh.Index, pairCap int, minSimilarity float64) clonePairResult {
	seen := make(map[PairKey]bool)
	sigMap := buildSignatureMap(entries)
	result := clonePairResult{clonedFunc: make(map[string]struct{})}

	for _, entry := range entries {
		candidates, err := idx.QueryThreshold(entry.sig, minSimilarity)
		if err != nil {
			continue
		}

		result = matchCandidates(entry, candidates, sigMap, seen, result, pairCap, minSimilarity)
	}

	sort.Slice(result.pairs, func(i, j int) bool {
		return result.pairs[i].Similarity > result.pairs[j].Similarity
	})

	return result
}

// buildSignatureMap creates a name-to-signature lookup from entries.
func buildSignatureMap(entries []funcEntry) map[string]*minhash.Signature {
	sigMap := make(map[string]*minhash.Signature, len(entries))

	for _, entry := range entries {
		sigMap[entry.name] = entry.sig
	}

	return sigMap
}

// matchCandidates processes LSH candidates for a single entry and appends matching pairs.
// totalCount tracks ALL valid pairs found. pairCap limits the stored slice (0 = unlimited).
func matchCandidates(
	entry funcEntry,
	candidates []string,
	sigMap map[string]*minhash.Signature,
	seen map[PairKey]bool,
	result clonePairResult,
	pairCap int,
	minSimilarity float64,
) clonePairResult {
	for _, candidateID := range candidates {
		if candidateID == entry.name {
			continue
		}

		key := clonePairKey(entry.name, candidateID)
		if seen[key] {
			continue
		}

		seen[key] = true

		pair, ok := computeClonePair(entry, candidateID, sigMap, minSimilarity)
		if ok {
			result.totalCount++
			result.typeDistribution.increment(pair.CloneType)
			result.clonedFunc[pair.FuncA] = struct{}{}
			result.clonedFunc[pair.FuncB] = struct{}{}

			if pairCap <= 0 || len(result.pairs) < pairCap {
				result.pairs = append(result.pairs, pair)
			}
		}
	}

	return result
}

// computeClonePair computes a clone pair between an entry and a candidate.
func computeClonePair(entry funcEntry, candidateID string, sigMap map[string]*minhash.Signature, minSimilarity float64) (ClonePair, bool) {
	candidateSig := sigMap[candidateID]
	if candidateSig == nil {
		return ClonePair{}, false
	}

	similarity, err := entry.sig.Similarity(candidateSig)
	if err != nil {
		return ClonePair{}, false
	}

	if similarity < minSimilarity {
		return ClonePair{}, false
	}

	return ClonePair{
		FuncA:      entry.name,
		FuncB:      candidateID,
		Similarity: similarity,
		CloneType:  classifyCloneType(similarity),
	}, true
}

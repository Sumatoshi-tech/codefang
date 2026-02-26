package clones

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/lsh"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// Visitor implements the AnalysisVisitor interface for clone detection.
// It collects function nodes during traversal and performs clone detection on exit.
type Visitor struct {
	functions []*node.Node
	shingler  *Shingler
}

// NewVisitor creates a new clone detection Visitor.
func NewVisitor() *Visitor {
	return &Visitor{
		shingler: NewShingler(defaultShingleSize),
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

// GetReport returns the clone detection report.
func (v *Visitor) GetReport() analyze.Report {
	if len(v.functions) == 0 {
		return buildEmptyReport(msgNoFunctions)
	}

	pairs := v.detectClones()

	return buildVisitorReport(len(v.functions), pairs)
}

// detectClones builds MinHash signatures and uses LSH to find clone pairs.
func (v *Visitor) detectClones() []ClonePair {
	entries := v.buildSignatures()
	if len(entries) == 0 {
		return nil
	}

	idx, err := lsh.New(numBands, numRows)
	if err != nil {
		return nil
	}

	for _, entry := range entries {
		insertErr := idx.Insert(entry.name, entry.sig)
		if insertErr != nil {
			continue
		}
	}

	return findClonePairs(entries, idx)
}

// buildSignatures computes MinHash signatures for all collected functions.
func (v *Visitor) buildSignatures() []funcEntry {
	entries := make([]funcEntry, 0, len(v.functions))

	for _, fn := range v.functions {
		shingles := v.shingler.ExtractShingles(fn)
		if len(shingles) == 0 {
			continue
		}

		sig, err := minhash.New(numHashes)
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

// findClonePairs queries the LSH index and collects unique clone pairs.
func findClonePairs(entries []funcEntry, idx *lsh.Index) []ClonePair {
	seen := make(map[string]bool)

	var pairs []ClonePair

	sigMap := buildSignatureMap(entries)

	for _, entry := range entries {
		candidates, err := idx.QueryThreshold(entry.sig, similarityType3)
		if err != nil {
			continue
		}

		pairs = matchCandidates(entry, candidates, sigMap, seen, pairs)
	}

	sort.Slice(pairs, func(i, j int) bool {
		return pairs[i].Similarity > pairs[j].Similarity
	})

	return pairs
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
func matchCandidates(
	entry funcEntry,
	candidates []string,
	sigMap map[string]*minhash.Signature,
	seen map[string]bool,
	pairs []ClonePair,
) []ClonePair {
	for _, candidateID := range candidates {
		if candidateID == entry.name {
			continue
		}

		key := clonePairKey(entry.name, candidateID)
		if seen[key] {
			continue
		}

		seen[key] = true

		pair, ok := computeClonePair(entry, candidateID, sigMap)
		if ok {
			pairs = append(pairs, pair)
		}
	}

	return pairs
}

// computeClonePair computes a clone pair between an entry and a candidate.
func computeClonePair(entry funcEntry, candidateID string, sigMap map[string]*minhash.Signature) (ClonePair, bool) {
	candidateSig := sigMap[candidateID]
	if candidateSig == nil {
		return ClonePair{}, false
	}

	similarity, err := entry.sig.Similarity(candidateSig)
	if err != nil {
		return ClonePair{}, false
	}

	if similarity < similarityType3 {
		return ClonePair{}, false
	}

	return ClonePair{
		FuncA:      entry.name,
		FuncB:      candidateID,
		Similarity: similarity,
		CloneType:  classifyCloneType(similarity),
	}, true
}

// buildVisitorReport constructs the analysis report from visitor data.
func buildVisitorReport(totalFunctions int, pairs []ClonePair) analyze.Report {
	cloneRatio := computeCloneRatio(len(pairs), totalFunctions)
	message := cloneMessage(len(pairs))

	pairsForReport := make([]map[string]any, 0, len(pairs))

	for _, p := range pairs {
		pairsForReport = append(pairsForReport, map[string]any{
			"func_a":     p.FuncA,
			"func_b":     p.FuncB,
			"similarity": p.Similarity,
			"clone_type": p.CloneType,
		})
	}

	return analyze.Report{
		keyAnalyzerName:    analyzerName,
		keyTotalFunctions:  totalFunctions,
		keyTotalClonePairs: len(pairs),
		keyCloneRatio:      cloneRatio,
		keyClonePairs:      pairsForReport,
		keyMessage:         message,
	}
}

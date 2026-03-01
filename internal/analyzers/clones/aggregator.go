package clones

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/lsh"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/minhash"
)

// Aggregator collects per-file function signatures and performs
// global cross-file clone detection in GetResult().
type Aggregator struct {
	entries        []funcEntry
	totalFunctions int
}

// NewAggregator creates a new clone detection aggregator.
func NewAggregator() *Aggregator {
	return &Aggregator{}
}

// Aggregate extracts function signatures from per-file reports.
// Signatures are qualified with the source file path so that
// same-named functions across files are distinguishable.
func (a *Aggregator) Aggregate(results map[string]analyze.Report) {
	for _, report := range results {
		if report == nil {
			continue
		}

		a.totalFunctions += extractTotalFunctions(report)
		a.collectSignatures(report)
	}
}

// extractTotalFunctions reads the total functions count from a report.
func extractTotalFunctions(report analyze.Report) int {
	v, ok := report[keyTotalFunctions]
	if !ok {
		return 0
	}

	n, nOK := v.(int)
	if !nOK {
		return 0
	}

	return n
}

// collectSignatures extracts function signatures from a single report
// and appends them to the aggregator's entries.
func (a *Aggregator) collectSignatures(report analyze.Report) {
	sigs, ok := report[keyFuncSignatures]
	if !ok {
		return
	}

	sigList, listOK := sigs.([]map[string]any)
	if !listOK {
		return
	}

	sourceFile, _ := extractSourceFile(sigList)

	for _, item := range sigList {
		name, nameOK := item["name"].(string)
		sig, sigOK := item["sig"].(*minhash.Signature)

		if !nameOK || !sigOK || sig == nil {
			continue
		}

		qualified := qualifyFuncName(name, sourceFile)

		a.entries = append(a.entries, funcEntry{
			name: qualified,
			sig:  sig,
		})
	}
}

// GetResult builds a global LSH index from all collected signatures,
// finds cross-file clone pairs, and computes metrics from global totals.
func (a *Aggregator) GetResult() analyze.Report {
	if a.totalFunctions == 0 {
		return buildEmptyReport(msgNoFunctions)
	}

	pairs := a.detectGlobalClones()

	cloneRatio := computeCloneRatio(len(pairs), a.totalFunctions)
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
		keyTotalFunctions:  a.totalFunctions,
		keyTotalClonePairs: len(pairs),
		keyCloneRatio:      cloneRatio,
		keyClonePairs:      pairsForReport,
		keyMessage:         message,
	}
}

// detectGlobalClones builds a single LSH index from all entries and finds clone pairs.
func (a *Aggregator) detectGlobalClones() []ClonePair {
	if len(a.entries) == 0 {
		return nil
	}

	idx, err := lsh.New(numBands, numRows)
	if err != nil {
		return nil
	}

	for _, entry := range a.entries {
		insertErr := idx.Insert(entry.name, entry.sig)
		if insertErr != nil {
			continue
		}
	}

	return findClonePairs(a.entries, idx)
}

// qualifyFuncName returns "sourceFile::name" if sourceFile is non-empty,
// or just "name" otherwise.
func qualifyFuncName(name, sourceFile string) string {
	if sourceFile == "" {
		return name
	}

	return fmt.Sprintf("%s::%s", sourceFile, name)
}

// extractSourceFile gets the _source_file from the first entry in a signature list.
// StampSourceFile in static.go sets this on every []map[string]any collection item.
func extractSourceFile(sigList []map[string]any) (string, bool) {
	if len(sigList) == 0 {
		return "", false
	}

	sf, ok := sigList[0]["_source_file"].(string)

	return sf, ok
}

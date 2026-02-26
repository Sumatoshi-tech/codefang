package clones

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
)

// Synergy test constants.
const (
	testFileA              = "pkg/foo.go"
	testFileB              = "pkg/bar.go"
	testFileC              = "pkg/baz.go"
	testHighCoupling       = 0.8
	testLowCoupling        = 0.2
	testHighSimilarity     = 0.9
	testLowSimilarity      = 0.6
	testSynergyFloatDelta  = 0.001
	testCouplingAtBoundary = 0.3
)

// TestComputeSynergy_BothEmpty verifies empty inputs produce nil.
func TestComputeSynergy_BothEmpty(t *testing.T) {
	t.Parallel()

	signals := ComputeSynergy(nil, nil)
	assert.Nil(t, signals)
}

// TestComputeSynergy_NoCoupling verifies nil coupling data produces nil.
func TestComputeSynergy_NoCoupling(t *testing.T) {
	t.Parallel()

	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: testHighSimilarity, CloneType: CloneType2},
	}

	signals := ComputeSynergy(nil, clonePairs)
	assert.Nil(t, signals)
}

// TestComputeSynergy_NoClones verifies nil clone pairs produce nil.
func TestComputeSynergy_NoClones(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 20, Strength: testHighCoupling},
	}

	signals := ComputeSynergy(couplingData, nil)
	assert.Nil(t, signals)
}

// TestComputeSynergy_MatchFound verifies signal emitted when both thresholds exceeded.
func TestComputeSynergy_MatchFound(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 20, Strength: testHighCoupling},
	}

	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: testHighSimilarity, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	require.Len(t, signals, 1)
	assert.Equal(t, testFileA, signals[0].FileA)
	assert.Equal(t, testFileB, signals[0].FileB)
	assert.InDelta(t, testHighCoupling, signals[0].CouplingStrength, testSynergyFloatDelta)
	assert.InDelta(t, testHighSimilarity, signals[0].CloneSimilarity, testSynergyFloatDelta)
	assert.NotEmpty(t, signals[0].Recommendation)
}

// TestComputeSynergy_CouplingBelowThreshold verifies no signal when coupling too low.
func TestComputeSynergy_CouplingBelowThreshold(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 2, Strength: testLowCoupling},
	}

	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: testHighSimilarity, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	assert.Nil(t, signals)
}

// TestComputeSynergy_SimilarityBelowThreshold verifies no signal when similarity too low.
func TestComputeSynergy_SimilarityBelowThreshold(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 20, Strength: testHighCoupling},
	}

	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: testLowSimilarity, CloneType: CloneType3},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	assert.Nil(t, signals)
}

// TestComputeSynergy_NoMatchingPair verifies no signal when pairs don't match.
func TestComputeSynergy_NoMatchingPair(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 20, Strength: testHighCoupling},
	}

	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileC, Similarity: testHighSimilarity, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	assert.Nil(t, signals)
}

// TestComputeSynergy_MultiplePairsSortedByStrength verifies sorting by combined strength.
func TestComputeSynergy_MultiplePairsSortedByStrength(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 10, Strength: 0.4},
		{File1: testFileA, File2: testFileC, CoChanges: 25, Strength: testHighCoupling},
	}

	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: testHighSimilarity, CloneType: CloneType2},
		{FuncA: testFileA, FuncB: testFileC, Similarity: 0.85, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	require.Len(t, signals, 2)

	// Second pair has higher combined strength: 0.8 * 0.85 = 0.68 > 0.4 * 0.9 = 0.36.
	assert.Equal(t, testFileA, signals[0].FileA)
	assert.Equal(t, testFileC, signals[0].FileB)
	assert.Equal(t, testFileA, signals[1].FileA)
	assert.Equal(t, testFileB, signals[1].FileB)
}

// TestComputeSynergy_ReversedPairOrder verifies canonical key handles reversed file order.
func TestComputeSynergy_ReversedPairOrder(t *testing.T) {
	t.Parallel()

	// Coupling has File1=A, File2=B but clone has FuncA=B, FuncB=A.
	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 20, Strength: testHighCoupling},
	}

	clonePairs := []ClonePair{
		{FuncA: testFileB, FuncB: testFileA, Similarity: testHighSimilarity, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	require.Len(t, signals, 1)
	assert.Equal(t, testFileA, signals[0].FileA)
}

// TestComputeSynergy_CouplingAtBoundary verifies boundary coupling (exactly at threshold).
func TestComputeSynergy_CouplingAtBoundary(t *testing.T) {
	t.Parallel()

	// Coupling strength exactly at threshold (0.3) — should NOT match (< not <=).
	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 5, Strength: testCouplingAtBoundary},
	}

	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: testHighSimilarity, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	assert.Nil(t, signals)
}

// TestComputeSynergy_SimilarityAtBoundary verifies boundary similarity (exactly at threshold).
func TestComputeSynergy_SimilarityAtBoundary(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 20, Strength: testHighCoupling},
	}

	// Similarity exactly at threshold (0.8) — should NOT match (< not <=).
	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: synergySimilarityThreshold, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	assert.Nil(t, signals)
}

// TestComputeSynergy_MaxSimilarityUsed verifies max similarity from multiple clone pairs.
func TestComputeSynergy_MaxSimilarityUsed(t *testing.T) {
	t.Parallel()

	couplingData := []couples.FileCouplingData{
		{File1: testFileA, File2: testFileB, CoChanges: 20, Strength: testHighCoupling},
	}

	// Two clone pairs for same file pair — max similarity should be used.
	clonePairs := []ClonePair{
		{FuncA: testFileA, FuncB: testFileB, Similarity: testLowSimilarity, CloneType: CloneType3},
		{FuncA: testFileA, FuncB: testFileB, Similarity: testHighSimilarity, CloneType: CloneType2},
	}

	signals := ComputeSynergy(couplingData, clonePairs)
	require.Len(t, signals, 1)
	assert.InDelta(t, testHighSimilarity, signals[0].CloneSimilarity, testSynergyFloatDelta)
}

// TestBuildRecommendation verifies recommendation message format.
func TestBuildRecommendation(t *testing.T) {
	t.Parallel()

	rec := buildRecommendation(testFileA, testFileB)
	assert.Contains(t, rec, testFileA)
	assert.Contains(t, rec, testFileB)
	assert.Contains(t, rec, "extract")
}

// TestSortSignalsByStrength verifies sorting by combined strength.
func TestSortSignalsByStrength(t *testing.T) {
	t.Parallel()

	signals := []RefactoringSignal{
		{CouplingStrength: 0.4, CloneSimilarity: testHighSimilarity},
		{CouplingStrength: testHighCoupling, CloneSimilarity: 0.85},
		{CouplingStrength: 0.5, CloneSimilarity: testHighSimilarity},
	}

	sortSignalsByStrength(signals)
	assert.InDelta(t, testHighCoupling, signals[0].CouplingStrength, testSynergyFloatDelta)
	assert.InDelta(t, 0.5, signals[1].CouplingStrength, testSynergyFloatDelta)
	assert.InDelta(t, 0.4, signals[2].CouplingStrength, testSynergyFloatDelta)
}

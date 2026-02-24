package devs

import (
	"context"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/identity"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

func TestAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	d := NewAnalyzer()
	facts := map[string]any{
		ConfigDevsConsiderEmptyCommits:                  true,
		identity.FactIdentityDetectorReversedPeopleDict: []string{"dev1"},
		pkgplumbing.FactTickSize:                        12 * time.Hour,
	}

	err := d.Configure(facts)
	require.NoError(t, err)
	assert.True(t, d.ConsiderEmptyCommits)
	assert.Len(t, d.reversedPeopleDict, 1)
	assert.Equal(t, 12*time.Hour, d.tickSize)
}

func TestAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	d := NewAnalyzer()

	err := d.Initialize(nil)
	require.NoError(t, err)
	assert.Equal(t, 24*time.Hour, d.tickSize)
	assert.NotNil(t, d.merges)
}

func TestAnalyzer_Consume_ReturnsTCWithCommitDevData(t *testing.T) {
	t.Parallel()

	d := newTestDevAnalyzer()

	hash1 := gitlib.NewHash("1111111111111111111111111111111111111111")
	change1 := &gitlib.Change{
		Action: gitlib.Insert,
		To:     gitlib.ChangeEntry{Name: "test.go", Hash: hash1},
	}
	d.TreeDiff.Changes = gitlib.Changes{change1}
	d.Ticks.Tick = 0
	d.Identity.AuthorID = 0
	d.Languages.SetLanguagesForTest(map[gitlib.Hash]string{hash1: "Go"})
	d.LineStats.LineStats = map[gitlib.ChangeEntry]pkgplumbing.LineStats{
		change1.To: {Added: 10, Removed: 3, Changed: 2},
	}

	commitHash := gitlib.NewHash("c100000000000000000000000000000000000001")
	commit := gitlib.NewTestCommit(
		commitHash,
		gitlib.TestSignature("dev", "dev@test.com"),
		"test commit",
	)

	tc, err := d.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cdd, ok := tc.Data.(*CommitDevData)
	require.True(t, ok, "TC.Data should be *CommitDevData")
	assert.Equal(t, 1, cdd.Commits)
	assert.Equal(t, 10, cdd.Added)
	assert.Equal(t, 3, cdd.Removed)
	assert.Equal(t, 2, cdd.Changed)
	assert.Equal(t, 10, cdd.Languages["Go"].Added)
	assert.Equal(t, commitHash, tc.CommitHash)
}

func TestAnalyzer_Consume_EmptyCommitIgnored(t *testing.T) {
	t.Parallel()

	d := newTestDevAnalyzer()
	d.TreeDiff.Changes = gitlib.Changes{}

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c200000000000000000000000000000000000002"),
		gitlib.TestSignature("dev", "dev@test.com"),
		"empty",
	)

	tc, err := d.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)
	assert.Nil(t, tc.Data, "empty commit should return nil TC data")
}

func TestAnalyzer_Consume_EmptyCommitConsidered(t *testing.T) {
	t.Parallel()

	d := newTestDevAnalyzer()
	d.ConsiderEmptyCommits = true
	d.TreeDiff.Changes = gitlib.Changes{}

	commit := gitlib.NewTestCommit(
		gitlib.NewHash("c300000000000000000000000000000000000003"),
		gitlib.TestSignature("dev", "dev@test.com"),
		"empty considered",
	)

	tc, err := d.Consume(context.Background(), &analyze.Context{Commit: commit})
	require.NoError(t, err)

	cdd, ok := tc.Data.(*CommitDevData)
	require.True(t, ok, "TC.Data should be *CommitDevData")
	assert.Equal(t, 1, cdd.Commits)
}

func TestAnalyzer_Consume_MergeDedup(t *testing.T) {
	t.Parallel()

	d := newTestDevAnalyzer()
	d.ConsiderEmptyCommits = true
	d.TreeDiff.Changes = gitlib.Changes{}

	mergeHash := gitlib.NewHash("m100000000000000000000000000000000000001")
	merge := gitlib.NewTestCommit(
		mergeHash,
		gitlib.TestSignature("dev", "dev@test.com"),
		"merge",
		gitlib.NewHash("p100000000000000000000000000000000000001"),
		gitlib.NewHash("p200000000000000000000000000000000000002"),
	)

	// First merge: processed (IsMerge=true so line stats skipped).
	tc1, err := d.Consume(context.Background(), &analyze.Context{Commit: merge, IsMerge: true})
	require.NoError(t, err)
	assert.NotNil(t, tc1.Data)
	assert.True(t, d.merges[mergeHash])

	// Second merge: deduped (already seen hash).
	tc2, err := d.Consume(context.Background(), &analyze.Context{Commit: merge, IsMerge: true})
	require.NoError(t, err)
	assert.Nil(t, tc2.Data, "duplicate merge should be skipped")
}

func TestAnalyzer_Fork(t *testing.T) {
	t.Parallel()

	d := newTestDevAnalyzer()

	clones := d.Fork(2)
	require.Len(t, clones, 2)

	for i, clone := range clones {
		c, ok := clone.(*Analyzer)
		require.True(t, ok, "clone %d should be *Analyzer", i)
		assert.NotNil(t, c.Identity)
		assert.NotNil(t, c.TreeDiff)
		assert.NotNil(t, c.Ticks)
		assert.NotNil(t, c.Languages)
		assert.NotNil(t, c.LineStats)
	}
}

func TestAnalyzer_Merge_IsNoOp(t *testing.T) {
	t.Parallel()

	d := newTestDevAnalyzer()
	d.Merge(nil)
	d.Merge([]analyze.HistoryAnalyzer{})
}

func TestAnalyzer_SequentialOnly(t *testing.T) {
	t.Parallel()

	d := NewAnalyzer()
	assert.True(t, d.SequentialOnly())
}

func TestAnalyzer_Misc(t *testing.T) {
	t.Parallel()

	d := NewAnalyzer()
	assert.NotEmpty(t, d.Name())
	assert.NotEmpty(t, d.Flag())
	assert.NotEmpty(t, d.Description())
	assert.NotEmpty(t, d.ListConfigurationOptions())
}

func TestAnalyzer_NewAggregator(t *testing.T) {
	t.Parallel()

	d := NewAnalyzer()
	require.NoError(t, d.Configure(map[string]any{
		identity.FactIdentityDetectorReversedPeopleDict: []string{"Alice", "Bob"},
		pkgplumbing.FactTickSize:                        24 * time.Hour,
	}))

	agg := d.AggregatorFn(analyze.AggregatorOptions{SpillBudget: 1 << 20})
	require.NotNil(t, agg)
}

func TestExtractCommitTimeSeries_Devs(t *testing.T) {
	t.Parallel()

	d := NewAnalyzer()

	report := analyze.Report{
		"CommitDevData": map[string]*CommitDevData{
			"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa": {
				Commits: 1, Added: 100, Removed: 20, Changed: 5,
			},
			"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb": {
				Commits: 1, Added: 50, Removed: 10, Changed: 3,
			},
		},
	}

	result := d.ExtractCommitTimeSeries(report)
	require.Len(t, result, 2)

	statsA, ok := result["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]
	require.True(t, ok)

	statsMap, ok := statsA.(map[string]any)
	require.True(t, ok)
	assert.Equal(t, 1, statsMap["commits"])
	assert.Equal(t, 100, statsMap["lines_added"])
	assert.Equal(t, 20, statsMap["lines_removed"])
	assert.Equal(t, 80, statsMap["net_change"])
}

func TestAggregateCommitsToTicks_Basic(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	commitDevData := map[string]*CommitDevData{
		h1.String(): {
			Commits: 1, Added: 20, Removed: 5, Changed: 3, AuthorID: 1,
			Languages: map[string]pkgplumbing.LineStats{"Go": {Added: 20, Removed: 5, Changed: 3}},
		},
		h2.String(): {
			Commits: 1, Added: 10, Removed: 3, Changed: 2, AuthorID: 2,
			Languages: map[string]pkgplumbing.LineStats{"Python": {Added: 10, Removed: 3, Changed: 2}},
		},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {h1, h2},
	}

	result := AggregateCommitsToTicks(commitDevData, commitsByTick)
	require.Len(t, result, 1)
	require.Len(t, result[0], 2)

	dt1 := result[0][1]
	require.NotNil(t, dt1)
	assert.Equal(t, 1, dt1.Commits)
	assert.Equal(t, 20, dt1.Added)
	assert.Equal(t, 5, dt1.Removed)

	dt2 := result[0][2]
	require.NotNil(t, dt2)
	assert.Equal(t, 1, dt2.Commits)
	assert.Equal(t, 10, dt2.Added)
}

func TestAggregateCommitsToTicks_SameAuthorMultipleCommits(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	commitDevData := map[string]*CommitDevData{
		h1.String(): {Commits: 1, Added: 20, Removed: 5, AuthorID: 1},
		h2.String(): {Commits: 1, Added: 10, Removed: 3, AuthorID: 1},
	}
	commitsByTick := map[int][]gitlib.Hash{
		0: {h1, h2},
	}

	result := AggregateCommitsToTicks(commitDevData, commitsByTick)
	require.Len(t, result, 1)
	require.Len(t, result[0], 1)

	dt := result[0][1]
	assert.Equal(t, 2, dt.Commits)
	assert.Equal(t, 30, dt.Added)
	assert.Equal(t, 8, dt.Removed)
}

func TestAggregateCommitsToTicks_EmptyInputs(t *testing.T) {
	t.Parallel()

	assert.Nil(t, AggregateCommitsToTicks(nil, map[int][]gitlib.Hash{0: {}}))
	assert.Nil(t, AggregateCommitsToTicks(map[string]*CommitDevData{"a": {}}, nil))
}

func TestComputeAllMetrics_FromCommitData(t *testing.T) {
	t.Parallel()

	h1 := gitlib.NewHash("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
	h2 := gitlib.NewHash("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")

	report := analyze.Report{
		"CommitDevData": map[string]*CommitDevData{
			h1.String(): {
				Commits: 1, Added: 20, Removed: 5, Changed: 3, AuthorID: 0,
				Languages: map[string]pkgplumbing.LineStats{"Go": {Added: 20, Removed: 5, Changed: 3}},
			},
			h2.String(): {
				Commits: 1, Added: 10, Removed: 3, Changed: 2, AuthorID: 1,
				Languages: map[string]pkgplumbing.LineStats{"Python": {Added: 10, Removed: 3, Changed: 2}},
			},
		},
		"CommitsByTick": map[int][]gitlib.Hash{
			0: {h1},
			1: {h2},
		},
		"ReversedPeopleDict": []string{"Alice", "Bob"},
		"TickSize":           24 * time.Hour,
	}

	computed, err := ComputeAllMetrics(report)
	require.NoError(t, err)
	assert.Len(t, computed.Developers, 2)
	assert.Equal(t, 2, computed.Aggregate.TotalCommits)
}

// newTestDevAnalyzer creates an analyzer with plumbing dependencies for Consume tests.
func newTestDevAnalyzer() *Analyzer {
	langs := &plumbing.LanguagesDetectionAnalyzer{}
	langs.SetLanguagesForTest(map[gitlib.Hash]string{})

	d := NewAnalyzer()
	d.Identity = &plumbing.IdentityDetector{}
	d.TreeDiff = &plumbing.TreeDiffAnalyzer{}
	d.Ticks = &plumbing.TicksSinceStart{}
	d.Languages = langs
	d.LineStats = &plumbing.LinesStatsCalculator{}
	d.merges = map[gitlib.Hash]bool{}
	d.tickSize = 24 * time.Hour

	return d
}

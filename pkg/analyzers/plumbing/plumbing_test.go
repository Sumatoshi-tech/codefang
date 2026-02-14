package plumbing

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestTreeDiffAnalyzer_Name(t *testing.T) {
	t.Parallel()

	td := &TreeDiffAnalyzer{}
	if td.Name() == "" {
		t.Error("Name empty")
	}
}

func TestTreeDiffAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	td := &TreeDiffAnalyzer{}
	err := td.Configure(nil)
	require.NoError(t, err)
}

func TestTreeDiffAnalyzer_Initialize(t *testing.T) {
	t.Parallel()

	td := &TreeDiffAnalyzer{}
	err := td.Initialize(nil)
	require.NoError(t, err)
}

func TestIdentityDetector_Name(t *testing.T) {
	t.Parallel()

	id := &IdentityDetector{}
	if id.Name() == "" {
		t.Error("Name empty")
	}
}

func TestIdentityDetector_Configure(t *testing.T) {
	t.Parallel()

	id := &IdentityDetector{}
	err := id.Configure(nil)
	require.NoError(t, err)
}

func TestBlobCacheAnalyzer_Name(t *testing.T) {
	t.Parallel()

	bc := &BlobCacheAnalyzer{}
	if bc.Name() == "" {
		t.Error("Name empty")
	}
}

func TestBlobCacheAnalyzer_Configure(t *testing.T) {
	t.Parallel()

	bc := &BlobCacheAnalyzer{}
	err := bc.Configure(nil)
	require.NoError(t, err)
}

func TestFileDiffAnalyzer_Name(t *testing.T) {
	t.Parallel()

	fd := &FileDiffAnalyzer{}
	if fd.Name() == "" {
		t.Error("Name empty")
	}
}

func TestLinesStatsCalculator_Name(t *testing.T) {
	t.Parallel()

	ls := &LinesStatsCalculator{}
	if ls.Name() == "" {
		t.Error("Name empty")
	}
}

func TestLanguagesDetectionAnalyzer_Name(t *testing.T) {
	t.Parallel()

	ld := &LanguagesDetectionAnalyzer{}
	if ld.Name() == "" {
		t.Error("Name empty")
	}
}

func TestTicksSinceStart_Name(t *testing.T) {
	t.Parallel()

	ts := &TicksSinceStart{}
	if ts.Name() == "" {
		t.Error("Name empty")
	}
}

func TestUASTChangesAnalyzer_Name(t *testing.T) {
	t.Parallel()

	ua := &UASTChangesAnalyzer{}
	if ua.Name() == "" {
		t.Error("Name empty")
	}
}

func TestChangeEntry_Hash(t *testing.T) {
	t.Parallel()

	hash := gitlib.NewHash("1111111111111111111111111111111111111111")

	ce := gitlib.ChangeEntry{Name: "test.go", Hash: hash}
	if ce.Hash != hash {
		t.Error("Hash mismatch")
	}

	if ce.Name != "test.go" {
		t.Error("Name mismatch")
	}
}

// TestTreeDiff_filterChanges_prefixBlacklist verifies blacklist uses path prefix match only.
func TestTreeDiff_filterChanges_prefixBlacklist(t *testing.T) {
	t.Parallel()

	hash := gitlib.NewHash("1111111111111111111111111111111111111111")
	td := &TreeDiffAnalyzer{
		SkipFiles: []string{"vendor/"},
		Languages: map[string]bool{allLanguages: true},
	}

	changes := gitlib.Changes{
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "vendor/foo.go", Hash: hash}},
		{Action: gitlib.Modify, To: gitlib.ChangeEntry{Name: "pkg/bar.go", Hash: hash}},
	}
	filtered := td.filterChanges(changes)
	require.Len(t, filtered, 1)
	require.Equal(t, "pkg/bar.go", filtered[0].To.Name)
}

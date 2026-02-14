package devs

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/streaming"
)

func TestHistoryAnalyzer_ImplementsHibernatable(t *testing.T) {
	t.Parallel()

	var _ streaming.Hibernatable = (*HistoryAnalyzer)(nil)
}

func TestHibernate_ClearsMergesMap(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithState()

	// Verify merges has data before hibernate.
	if len(analyzer.merges) == 0 {
		t.Fatal("test setup error: merges should not be empty")
	}

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Verify merges is cleared after hibernate.
	if len(analyzer.merges) != 0 {
		t.Errorf("merges not cleared: got %d entries, want 0", len(analyzer.merges))
	}
}

func TestHibernate_PreservesTicksData(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithState()

	// Capture ticks state before hibernate.
	ticksBefore := len(analyzer.tickData)
	commitsBefore := analyzer.tickData[0][1].Commits

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Verify ticks data is preserved.
	if len(analyzer.tickData) != ticksBefore {
		t.Errorf("ticks count changed: got %d, want %d", len(analyzer.tickData), ticksBefore)
	}

	if analyzer.tickData[0][1].Commits != commitsBefore {
		t.Errorf("commits changed: got %d, want %d", analyzer.tickData[0][1].Commits, commitsBefore)
	}
}

func TestBoot_InitializesMergesMap(t *testing.T) {
	t.Parallel()

	analyzer := &HistoryAnalyzer{
		tickData: make(map[int]map[int]*DevTick),
		merges:   nil, // Simulate nil state.
	}

	err := analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// Verify merges map is initialized.
	if analyzer.merges == nil {
		t.Error("merges should be initialized after Boot()")
	}
}

func TestBoot_PreservesExistingMerges(t *testing.T) {
	t.Parallel()

	hash := gitlib.NewHash("0123456789abcdef0123456789abcdef01234567")
	analyzer := &HistoryAnalyzer{
		tickData: make(map[int]map[int]*DevTick),
		merges: map[gitlib.Hash]bool{
			hash: true,
		},
	}

	err := analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// Verify existing merges are preserved.
	if !analyzer.merges[hash] {
		t.Error("existing merge entry should be preserved after Boot()")
	}
}

func TestHibernate_Boot_RoundTrip(t *testing.T) {
	t.Parallel()

	analyzer := createAnalyzerWithState()

	// Capture original ticks state.
	originalTicks := analyzer.tickData[0][1].Commits
	originalAdded := analyzer.tickData[0][1].Added

	// Hibernate clears merges.
	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Boot prepares for next chunk.
	err = analyzer.Boot()
	if err != nil {
		t.Fatalf("Boot() failed: %v", err)
	}

	// Verify ticks data survived round trip.
	if analyzer.tickData[0][1].Commits != originalTicks {
		t.Errorf("commits changed after round trip: got %d, want %d",
			analyzer.tickData[0][1].Commits, originalTicks)
	}

	if analyzer.tickData[0][1].Added != originalAdded {
		t.Errorf("added changed after round trip: got %d, want %d",
			analyzer.tickData[0][1].Added, originalAdded)
	}

	// Verify merges map is ready for new entries.
	if analyzer.merges == nil {
		t.Error("merges should be ready after round trip")
	}
}

func TestHibernate_MemoryReduction(t *testing.T) {
	t.Parallel()

	// Create analyzer with large merges map.
	const mergeCount = 1000

	analyzer := &HistoryAnalyzer{
		tickData: make(map[int]map[int]*DevTick),
		merges:   make(map[gitlib.Hash]bool, mergeCount),
	}

	// Populate merges with test data.
	for i := range mergeCount {
		hash := gitlib.NewHash("0123456789abcdef0123456789abcdef01234567")
		// Modify hash to create unique entries.
		analyzer.merges[gitlib.Hash{byte(i), byte(i >> 8)}] = true
		_ = hash // avoid unused variable.
	}

	mergesBefore := len(analyzer.merges)
	if mergesBefore != mergeCount {
		t.Fatalf("test setup: expected %d merges, got %d", mergeCount, mergesBefore)
	}

	err := analyzer.Hibernate()
	if err != nil {
		t.Fatalf("Hibernate() failed: %v", err)
	}

	// Verify memory reduction.
	if len(analyzer.merges) != 0 {
		t.Errorf("merges should be empty after hibernate, got %d", len(analyzer.merges))
	}
}

// BenchmarkHibernate measures hibernation performance with realistic merge counts.
func BenchmarkHibernate(b *testing.B) {
	const mergeCount = 10000 // Simulate large repo with many merges.

	analyzer := &HistoryAnalyzer{
		tickData: make(map[int]map[int]*DevTick),
		merges:   make(map[gitlib.Hash]bool, mergeCount),
	}

	// Populate merges.
	for i := range mergeCount {
		analyzer.merges[gitlib.Hash{byte(i), byte(i >> 8), byte(i >> 16)}] = true
	}

	b.ReportAllocs()

	for b.Loop() {
		err := analyzer.Hibernate()
		if err != nil {
			b.Fatal(err)
		}
		// Re-populate for next iteration.
		for i := range mergeCount {
			analyzer.merges[gitlib.Hash{byte(i), byte(i >> 8), byte(i >> 16)}] = true
		}
	}
}

// BenchmarkBoot measures boot performance.
func BenchmarkBoot(b *testing.B) {
	analyzer := &HistoryAnalyzer{
		tickData: make(map[int]map[int]*DevTick),
		merges:   nil,
	}

	b.ReportAllocs()

	for b.Loop() {
		analyzer.merges = nil

		err := analyzer.Boot()
		if err != nil {
			b.Fatal(err)
		}
	}
}

func createAnalyzerWithState() *HistoryAnalyzer {
	return &HistoryAnalyzer{
		tickData: map[int]map[int]*DevTick{
			0: {
				1: {
					LineStats: plumbing.LineStats{Added: 100, Removed: 20, Changed: 10},
					Commits:   5,
					Languages: map[string]plumbing.LineStats{
						"go": {Added: 80, Removed: 15, Changed: 8},
					},
				},
			},
		},
		merges: map[gitlib.Hash]bool{
			gitlib.NewHash("0123456789abcdef0123456789abcdef01234567"): true,
			gitlib.NewHash("fedcba9876543210fedcba9876543210fedcba98"): true,
		},
		reversedPeopleDict: []string{"Alice", "Bob"},
	}
}

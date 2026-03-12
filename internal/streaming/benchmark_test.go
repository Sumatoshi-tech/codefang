package streaming_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
)

// Checkpoint Save Benchmarks.

func BenchmarkCheckpointSave_Burndown(b *testing.B) {
	dir := b.TempDir()
	analyzer := &burndown.HistoryAnalyzer{
		Granularity: burndown.DefaultBurndownGranularity,
		Sampling:    burndown.DefaultBurndownSampling,
		Goroutines:  2,
	}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		err = analyzer.SaveCheckpoint(dir)
		if err != nil {
			b.Fatalf("SaveCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointSave_Couples(b *testing.B) {
	dir := b.TempDir()
	analyzer := &couples.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		err = analyzer.SaveCheckpoint(dir)
		if err != nil {
			b.Fatalf("SaveCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointSave_FileHistory(b *testing.B) {
	dir := b.TempDir()
	analyzer := filehistory.NewAnalyzer()

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		err = analyzer.SaveCheckpoint(dir)
		if err != nil {
			b.Fatalf("SaveCheckpoint failed: %v", err)
		}
	}
}

// Checkpoint Load Benchmarks.

func BenchmarkCheckpointLoad_Burndown(b *testing.B) {
	dir := b.TempDir()
	analyzer := &burndown.HistoryAnalyzer{
		Granularity: burndown.DefaultBurndownGranularity,
		Sampling:    burndown.DefaultBurndownSampling,
		Goroutines:  2,
	}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	err = analyzer.SaveCheckpoint(dir)
	if err != nil {
		b.Fatalf("SaveCheckpoint failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		loaded := &burndown.HistoryAnalyzer{}

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointLoad_Couples(b *testing.B) {
	dir := b.TempDir()
	analyzer := &couples.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	err = analyzer.SaveCheckpoint(dir)
	if err != nil {
		b.Fatalf("SaveCheckpoint failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		loaded := &couples.HistoryAnalyzer{}

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointLoad_FileHistory(b *testing.B) {
	dir := b.TempDir()
	analyzer := filehistory.NewAnalyzer()

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	err = analyzer.SaveCheckpoint(dir)
	if err != nil {
		b.Fatalf("SaveCheckpoint failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		loaded := filehistory.NewAnalyzer()

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}

// Hibernate Benchmarks.

func BenchmarkHibernate_Burndown(b *testing.B) {
	analyzer := &burndown.HistoryAnalyzer{
		Granularity: burndown.DefaultBurndownGranularity,
		Sampling:    burndown.DefaultBurndownSampling,
		Goroutines:  2,
	}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		err = analyzer.Hibernate()
		if err != nil {
			b.Fatalf("Hibernate failed: %v", err)
		}

		err = analyzer.Boot()
		if err != nil {
			b.Fatalf("Boot failed: %v", err)
		}
	}
}

func BenchmarkHibernate_Couples(b *testing.B) {
	analyzer := &couples.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		err = analyzer.Hibernate()
		if err != nil {
			b.Fatalf("Hibernate failed: %v", err)
		}

		err = analyzer.Boot()
		if err != nil {
			b.Fatalf("Boot failed: %v", err)
		}
	}
}

func BenchmarkHibernate_FileHistory(b *testing.B) {
	analyzer := filehistory.NewAnalyzer()

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		err = analyzer.Hibernate()
		if err != nil {
			b.Fatalf("Hibernate failed: %v", err)
		}

		err = analyzer.Boot()
		if err != nil {
			b.Fatalf("Boot failed: %v", err)
		}
	}
}

// Fork/Merge Benchmarks.

func BenchmarkFork_Devs(b *testing.B) {
	const numForks = 4

	analyzer := devs.NewAnalyzer()

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		_ = analyzer.Fork(numForks)
	}
}

func BenchmarkMerge_Devs(b *testing.B) {
	const numForks = 4

	analyzer := devs.NewAnalyzer()

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	forks := analyzer.Fork(numForks)

	b.ReportAllocs()

	for b.Loop() {
		analyzer.Merge(forks)
	}
}

func BenchmarkFork_Couples(b *testing.B) {
	const numForks = 4

	analyzer := &couples.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		_ = analyzer.Fork(numForks)
	}
}

func BenchmarkMerge_Couples(b *testing.B) {
	const numForks = 4

	analyzer := &couples.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	forks := analyzer.Fork(numForks)

	b.ReportAllocs()

	for b.Loop() {
		analyzer.Merge(forks)
	}
}

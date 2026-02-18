package streaming_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
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

func BenchmarkCheckpointSave_Devs(b *testing.B) {
	dir := b.TempDir()
	analyzer := &devs.HistoryAnalyzer{}

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
	analyzer := &filehistory.Analyzer{}

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

func BenchmarkCheckpointSave_Shotness(b *testing.B) {
	dir := b.TempDir()
	analyzer := &shotness.HistoryAnalyzer{}

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

func BenchmarkCheckpointSave_Sentiment(b *testing.B) {
	dir := b.TempDir()
	analyzer := &sentiment.HistoryAnalyzer{}

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

func BenchmarkCheckpointSave_Imports(b *testing.B) {
	dir := b.TempDir()
	analyzer := &imports.HistoryAnalyzer{}

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

func BenchmarkCheckpointSave_Typos(b *testing.B) {
	dir := b.TempDir()
	analyzer := &typos.HistoryAnalyzer{}

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

func BenchmarkCheckpointLoad_Devs(b *testing.B) {
	dir := b.TempDir()
	analyzer := &devs.HistoryAnalyzer{}

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
		loaded := &devs.HistoryAnalyzer{}

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
	analyzer := &filehistory.Analyzer{}

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
		loaded := &filehistory.Analyzer{}

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointLoad_Shotness(b *testing.B) {
	dir := b.TempDir()
	analyzer := &shotness.HistoryAnalyzer{}

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
		loaded := &shotness.HistoryAnalyzer{}

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointLoad_Sentiment(b *testing.B) {
	dir := b.TempDir()
	analyzer := &sentiment.HistoryAnalyzer{}

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
		loaded := &sentiment.HistoryAnalyzer{}

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointLoad_Imports(b *testing.B) {
	dir := b.TempDir()
	analyzer := &imports.HistoryAnalyzer{}

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
		loaded := &imports.HistoryAnalyzer{}

		err = loaded.LoadCheckpoint(dir)
		if err != nil {
			b.Fatalf("LoadCheckpoint failed: %v", err)
		}
	}
}

func BenchmarkCheckpointLoad_Typos(b *testing.B) {
	dir := b.TempDir()
	analyzer := &typos.HistoryAnalyzer{}

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
		loaded := &typos.HistoryAnalyzer{}

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

func BenchmarkHibernate_Devs(b *testing.B) {
	analyzer := &devs.HistoryAnalyzer{}

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
	analyzer := &filehistory.Analyzer{}

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

func BenchmarkHibernate_Shotness(b *testing.B) {
	analyzer := &shotness.HistoryAnalyzer{}

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

func BenchmarkHibernate_Sentiment(b *testing.B) {
	analyzer := &sentiment.HistoryAnalyzer{}

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

func BenchmarkHibernate_Imports(b *testing.B) {
	analyzer := &imports.HistoryAnalyzer{}

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

func BenchmarkHibernate_Typos(b *testing.B) {
	analyzer := &typos.HistoryAnalyzer{}

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

	analyzer := &devs.HistoryAnalyzer{}

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

	analyzer := &devs.HistoryAnalyzer{}

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

func BenchmarkFork_Sentiment(b *testing.B) {
	const numForks = 4

	analyzer := &sentiment.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		_ = analyzer.Fork(numForks)
	}
}

func BenchmarkMerge_Sentiment(b *testing.B) {
	const numForks = 4

	analyzer := &sentiment.HistoryAnalyzer{}

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

func BenchmarkFork_Shotness(b *testing.B) {
	const numForks = 4

	analyzer := &shotness.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		_ = analyzer.Fork(numForks)
	}
}

func BenchmarkMerge_Shotness(b *testing.B) {
	const numForks = 4

	analyzer := &shotness.HistoryAnalyzer{}

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

func BenchmarkFork_Typos(b *testing.B) {
	const numForks = 4

	analyzer := &typos.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		_ = analyzer.Fork(numForks)
	}
}

func BenchmarkMerge_Typos(b *testing.B) {
	const numForks = 4

	analyzer := &typos.HistoryAnalyzer{}

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

func BenchmarkFork_Imports(b *testing.B) {
	const numForks = 4

	analyzer := &imports.HistoryAnalyzer{}

	err := analyzer.Initialize(nil)
	if err != nil {
		b.Fatalf("Initialize failed: %v", err)
	}

	b.ReportAllocs()

	for b.Loop() {
		_ = analyzer.Fork(numForks)
	}
}

func BenchmarkMerge_Imports(b *testing.B) {
	const numForks = 4

	analyzer := &imports.HistoryAnalyzer{}

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

package burndown

import (
	"context"
	"errors"
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

// Store record kind constants.
const (
	KindChartData = "chart_data"
	KindMetrics   = "metrics"
)

// ChartData holds pre-computed chart rendering data for store serialization.
type ChartData struct {
	GlobalHistory DenseHistory
	Sampling      int
	Granularity   int
	TickSize      int64 // nanoseconds, gob-safe.
	EndTime       int64 // unix nanoseconds, gob-safe.
}

// ErrUnexpectedAggregator indicates a type assertion failure for the aggregator.
var ErrUnexpectedAggregator = errors.New("unexpected aggregator type: expected *burndown.Aggregator")

// WriteToStoreFromAggregator implements analyze.DirectStoreWriter.
// It handles Collect() and accesses the aggregator state directly, avoiding the
// five deep clones in FlushTick. Pre-computes chart data and metrics:
//   - "chart_data": global DenseHistory + rendering metadata.
//   - "metrics": pre-computed ComputedMetrics (survival, interaction, aggregate).
func (b *HistoryAnalyzer) WriteToStoreFromAggregator(
	ctx context.Context,
	agg analyze.Aggregator,
	w analyze.ReportWriter,
) error {
	_ = ctx

	collectErr := agg.Collect()
	if collectErr != nil {
		return fmt.Errorf("collect: %w", collectErr)
	}

	ba, ok := agg.(*Aggregator)
	if !ok {
		return ErrUnexpectedAggregator
	}

	chartData := b.buildChartData(ba)

	chartErr := w.Write(KindChartData, chartData)
	if chartErr != nil {
		return fmt.Errorf("write %s: %w", KindChartData, chartErr)
	}

	metrics := b.computeMetricsFromAggregator(ba, chartData.GlobalHistory)

	metricsErr := w.Write(KindMetrics, metrics)
	if metricsErr != nil {
		return fmt.Errorf("write %s: %w", KindMetrics, metricsErr)
	}

	return nil
}

// buildChartData converts the global sparse history to dense and packages
// it with rendering metadata.
func (b *HistoryAnalyzer) buildChartData(agg *Aggregator) ChartData {
	globalDense := b.groupSparseHistory(agg.globalHistory, agg.lastTick)

	return ChartData{
		GlobalHistory: globalDense,
		Sampling:      agg.sampling,
		Granularity:   agg.granularity,
		TickSize:      int64(agg.tickSize),
		EndTime:       agg.endTime.UnixNano(),
	}
}

// computeMetricsFromAggregator pre-computes all metrics directly from aggregator
// state without materializing dense file histories.
func (b *HistoryAnalyzer) computeMetricsFromAggregator(
	agg *Aggregator,
	globalDense DenseHistory,
) *ComputedMetrics {
	reportData := &ReportData{
		GlobalHistory: globalDense,
		Sampling:      agg.sampling,
		Granularity:   agg.granularity,
		TickSize:      agg.tickSize,
	}

	globalSurvival := computeGlobalSurvival(reportData)
	aggregate := computeAggregate(reportData)

	// Override tracked counts from aggregator state directly.
	aggregate.TrackedFiles = len(agg.fileHistories)
	aggregate.TrackedDevelopers = len(agg.peopleHistories)

	// Compute developer survival one person at a time to avoid
	// holding all dense people histories in memory simultaneously.
	devSurvival := b.computeDevSurvivalStreaming(agg)

	// Compute interaction from sparse matrix (bounded by peopleNumber).
	interaction := computeInteractionFromSparse(agg.matrix, agg.reversedPeopleDict, agg.peopleNumber)

	// Compute file survival from ownership (no dense file histories needed).
	fileSurvival := computeFileSurvivalFromOwnership(agg.fileOwnership, agg.pathInterner, agg.reversedPeopleDict)

	return &ComputedMetrics{
		Aggregate:         aggregate,
		GlobalSurvival:    globalSurvival,
		FileSurvival:      fileSurvival,
		DeveloperSurvival: devSurvival,
		Interaction:       interaction,
	}
}

// computeDevSurvivalStreaming converts each person's sparse history to dense
// one at a time, computes survival, then discards the dense history.
func (b *HistoryAnalyzer) computeDevSurvivalStreaming(agg *Aggregator) []DeveloperSurvivalData {
	if len(agg.peopleHistories) == 0 {
		return nil
	}

	result := make([]DeveloperSurvivalData, 0, len(agg.peopleHistories))

	for devID, history := range agg.peopleHistories {
		if len(history) == 0 {
			continue
		}

		dense := b.groupSparseHistory(history, agg.lastTick)
		data := computeDeveloperSurvival(devID, dense, agg.reversedPeopleDict)
		result = append(result, data)
	}

	return result
}

// computeInteractionFromSparse converts the sparse matrix to dense and
// computes interaction data.
func computeInteractionFromSparse(
	matrix []map[int]int64,
	reversedNames []string,
	peopleNumber int,
) []InteractionData {
	if len(matrix) == 0 {
		return nil
	}

	denseMatrix := buildDenseMatrix(matrix, peopleNumber)

	return computeInteraction(InteractionInput{
		PeopleMatrix:       denseMatrix,
		ReversedPeopleDict: reversedNames,
	})
}

// computeFileSurvivalFromOwnership computes file survival from the aggregator's
// fileOwnership map without creating dense file histories.
func computeFileSurvivalFromOwnership(
	ownership map[PathID]map[int]int,
	interner *PathInterner,
	reversedNames []string,
) []FileSurvivalData {
	if len(ownership) == 0 {
		return nil
	}

	// Convert PathID-keyed ownership to string-keyed for computeFileSurvival.
	stringOwnership := make(map[string]map[int]int, len(ownership))

	for pathID, devMap := range ownership {
		name := interner.Lookup(pathID)
		if name == "" {
			continue
		}

		stringOwnership[name] = devMap
	}

	return computeFileSurvival(FileSurvivalInput{
		FileOwnership:      stringOwnership,
		ReversedPeopleDict: reversedNames,
	})
}

package halstead

import (
	"math"
)

// Halstead metrics divisor constants used in complexity calculations.
const (
	// DeliveredBugsValue is the divisor for estimating delivered bugs (Volume / DeliveredBugsValue).
	DeliveredBugsValue = 3000.0
	// DifficultyValue is the divisor used in the difficulty calculation (DistinctOperators / DifficultyValue).
	DifficultyValue = 2.0
	// TimeToProgramValue is the divisor for estimating time to program in seconds (Effort / TimeToProgramValue).
	TimeToProgramValue = 18.0
)

// MetricsCalculator handles all Halstead metrics calculations.
type MetricsCalculator struct{}

// NewMetricsCalculator creates a new metrics calculator.
func NewMetricsCalculator() *MetricsCalculator {
	return &MetricsCalculator{}
}

// CalculateHalsteadMetrics calculates all Halstead complexity measures.
func (mc *MetricsCalculator) CalculateHalsteadMetrics(metrics any) {
	var result *Metrics

	var fm *FunctionHalsteadMetrics

	switch v := metrics.(type) {
	case *Metrics:
		result = v
	case *FunctionHalsteadMetrics:
		fm = v
		result = &Metrics{
			DistinctOperators: fm.DistinctOperators,
			DistinctOperands:  fm.DistinctOperands,
			TotalOperators:    fm.TotalOperators,
			TotalOperands:     fm.TotalOperands,
		}
	}

	mc.calculateBasicMeasures(result)
	mc.calculateEstimatedLength(result)
	mc.calculateVolume(result)
	mc.calculateDifficulty(result)
	mc.calculateEffort(result)
	mc.calculateTimeAndBugs(result)

	if fm != nil {
		mc.updateFunctionMetrics(fm, result)
	}
}

// calculateBasicMeasures calculates basic Halstead measures.
func (mc *MetricsCalculator) calculateBasicMeasures(m *Metrics) {
	m.Vocabulary = m.DistinctOperators + m.DistinctOperands
	m.Length = m.TotalOperators + m.TotalOperands
}

// calculateEstimatedLength calculates the estimated length.
func (mc *MetricsCalculator) calculateEstimatedLength(m *Metrics) {
	if m.DistinctOperators > 0 {
		m.EstimatedLength += float64(m.DistinctOperators) * math.Log2(float64(m.DistinctOperators))
	}

	if m.DistinctOperands > 0 {
		m.EstimatedLength += float64(m.DistinctOperands) * math.Log2(float64(m.DistinctOperands))
	}
}

// calculateVolume calculates the volume.
func (mc *MetricsCalculator) calculateVolume(m *Metrics) {
	if m.Vocabulary > 0 {
		m.Volume = float64(m.Length) * math.Log2(float64(m.Vocabulary))
	}
}

// calculateDifficulty calculates the difficulty.
func (mc *MetricsCalculator) calculateDifficulty(m *Metrics) {
	if m.DistinctOperands > 0 {
		m.Difficulty = (float64(m.DistinctOperators) / DifficultyValue) * (float64(m.TotalOperands) / float64(m.DistinctOperands))
	}
}

// calculateEffort calculates the effort.
func (mc *MetricsCalculator) calculateEffort(m *Metrics) {
	m.Effort = m.Difficulty * m.Volume
}

// calculateTimeAndBugs calculates time to program and delivered bugs.
func (mc *MetricsCalculator) calculateTimeAndBugs(m *Metrics) {
	m.TimeToProgram = m.Effort / TimeToProgramValue
	m.DeliveredBugs = m.Volume / DeliveredBugsValue
}

// updateFunctionMetrics updates function metrics with calculated values.
func (mc *MetricsCalculator) updateFunctionMetrics(fm *FunctionHalsteadMetrics, metrics *Metrics) {
	fm.Vocabulary = metrics.Vocabulary
	fm.Length = metrics.Length
	fm.EstimatedLength = metrics.EstimatedLength
	fm.Volume = metrics.Volume
	fm.Difficulty = metrics.Difficulty
	fm.Effort = metrics.Effort
	fm.TimeToProgram = metrics.TimeToProgram
	fm.DeliveredBugs = metrics.DeliveredBugs
}

// SumMap sums all values in a map.
func (mc *MetricsCalculator) SumMap(m map[string]int) int {
	sum := 0
	for _, v := range m {
		sum += v
	}

	return sum
}

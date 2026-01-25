package halstead

import (
	"math"
)

// MetricsCalculator handles all Halstead metrics calculations
type MetricsCalculator struct{}

// NewMetricsCalculator creates a new metrics calculator
func NewMetricsCalculator() *MetricsCalculator {
	return &MetricsCalculator{}
}

// CalculateHalsteadMetrics calculates all Halstead complexity measures
func (mc *MetricsCalculator) CalculateHalsteadMetrics(metrics interface{}) {
	var m *HalsteadMetrics
	var fm *FunctionHalsteadMetrics

	switch v := metrics.(type) {
	case *HalsteadMetrics:
		m = v
	case *FunctionHalsteadMetrics:
		fm = v
		m = &HalsteadMetrics{
			DistinctOperators: fm.DistinctOperators,
			DistinctOperands:  fm.DistinctOperands,
			TotalOperators:    fm.TotalOperators,
			TotalOperands:     fm.TotalOperands,
		}
	}

	mc.calculateBasicMeasures(m)
	mc.calculateEstimatedLength(m)
	mc.calculateVolume(m)
	mc.calculateDifficulty(m)
	mc.calculateEffort(m)
	mc.calculateTimeAndBugs(m)

	if fm != nil {
		mc.updateFunctionMetrics(fm, m)
	}
}

// calculateBasicMeasures calculates basic Halstead measures
func (mc *MetricsCalculator) calculateBasicMeasures(m *HalsteadMetrics) {
	m.Vocabulary = m.DistinctOperators + m.DistinctOperands
	m.Length = m.TotalOperators + m.TotalOperands
}

// calculateEstimatedLength calculates the estimated length
func (mc *MetricsCalculator) calculateEstimatedLength(m *HalsteadMetrics) {
	if m.DistinctOperators > 0 {
		m.EstimatedLength += float64(m.DistinctOperators) * math.Log2(float64(m.DistinctOperators))
	}
	if m.DistinctOperands > 0 {
		m.EstimatedLength += float64(m.DistinctOperands) * math.Log2(float64(m.DistinctOperands))
	}
}

// calculateVolume calculates the volume
func (mc *MetricsCalculator) calculateVolume(m *HalsteadMetrics) {
	if m.Vocabulary > 0 {
		m.Volume = float64(m.Length) * math.Log2(float64(m.Vocabulary))
	}
}

// calculateDifficulty calculates the difficulty
func (mc *MetricsCalculator) calculateDifficulty(m *HalsteadMetrics) {
	if m.DistinctOperands > 0 {
		m.Difficulty = (float64(m.DistinctOperators) / 2.0) * (float64(m.TotalOperands) / float64(m.DistinctOperands))
	}
}

// calculateEffort calculates the effort
func (mc *MetricsCalculator) calculateEffort(m *HalsteadMetrics) {
	m.Effort = m.Difficulty * m.Volume
}

// calculateTimeAndBugs calculates time to program and delivered bugs
func (mc *MetricsCalculator) calculateTimeAndBugs(m *HalsteadMetrics) {
	m.TimeToProgram = m.Effort / 18.0
	m.DeliveredBugs = m.Volume / 3000.0
}

// updateFunctionMetrics updates function metrics with calculated values
func (mc *MetricsCalculator) updateFunctionMetrics(fm *FunctionHalsteadMetrics, m *HalsteadMetrics) {
	fm.Vocabulary = m.Vocabulary
	fm.Length = m.Length
	fm.EstimatedLength = m.EstimatedLength
	fm.Volume = m.Volume
	fm.Difficulty = m.Difficulty
	fm.Effort = m.Effort
	fm.TimeToProgram = m.TimeToProgram
	fm.DeliveredBugs = m.DeliveredBugs
}

// SumMap sums all values in a map
func (mc *MetricsCalculator) SumMap(m map[string]int) int {
	sum := 0
	for _, v := range m {
		sum += v
	}
	return sum
}

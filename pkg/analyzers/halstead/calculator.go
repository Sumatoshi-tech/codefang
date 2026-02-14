package halstead

import "math"

// MetricsCalculator handles Halstead metrics calculations.
type MetricsCalculator struct{}

// NewMetricsCalculator creates a new metrics calculator.
func NewMetricsCalculator() *MetricsCalculator {
	return &MetricsCalculator{}
}

// Halstead formula constants.
const (
	// TimeConstant is the standard constant used in time-to-program estimation (18 seconds).
	TimeConstant = 18.0
	// BugConstant is the standard constant used in delivered bugs estimation.
	BugConstant = 3000.0
	// BugExponent is the exponent used in delivered bugs calculation.
	BugExponent = 2.0 / 3.0
	// DifficultyDivisor is used in the difficulty formula: n1/2 * (N2/n2).
	DifficultyDivisor = 2.0
)

// halsteadGetter provides read access to Halstead operand/operator counts.
type halsteadGetter interface {
	getDistinctOperators() int
	getDistinctOperands() int
	getTotalOperators() int
	getTotalOperands() int
}

// halsteadBaseSetter provides write access to basic Halstead metrics.
type halsteadBaseSetter interface {
	setVocabulary(int)
	setLength(int)
	setEstimatedLength(float64)
	setVolume(float64)
}

// halsteadDerivedSetter provides write access to derived Halstead metrics.
type halsteadDerivedSetter interface {
	setDifficulty(float64)
	setEffort(float64)
	setTimeToProgram(float64)
	setDeliveredBugs(float64)
}

// halsteadMetrics combines all interfaces for Halstead calculation.
type halsteadMetrics interface {
	halsteadGetter
	halsteadBaseSetter
	halsteadDerivedSetter
}

// Implement interface for Metrics.
func (m *Metrics) getDistinctOperators() int    { return m.DistinctOperators }
func (m *Metrics) getDistinctOperands() int     { return m.DistinctOperands }
func (m *Metrics) getTotalOperators() int       { return m.TotalOperators }
func (m *Metrics) getTotalOperands() int        { return m.TotalOperands }
func (m *Metrics) setVocabulary(v int)          { m.Vocabulary = v }
func (m *Metrics) setLength(v int)              { m.Length = v }
func (m *Metrics) setEstimatedLength(v float64) { m.EstimatedLength = v }
func (m *Metrics) setVolume(v float64)          { m.Volume = v }
func (m *Metrics) setDifficulty(v float64)      { m.Difficulty = v }
func (m *Metrics) setEffort(v float64)          { m.Effort = v }
func (m *Metrics) setTimeToProgram(v float64)   { m.TimeToProgram = v }
func (m *Metrics) setDeliveredBugs(v float64)   { m.DeliveredBugs = v }

// Implement interface for FunctionHalsteadMetrics.
func (m *FunctionHalsteadMetrics) getDistinctOperators() int    { return m.DistinctOperators }
func (m *FunctionHalsteadMetrics) getDistinctOperands() int     { return m.DistinctOperands }
func (m *FunctionHalsteadMetrics) getTotalOperators() int       { return m.TotalOperators }
func (m *FunctionHalsteadMetrics) getTotalOperands() int        { return m.TotalOperands }
func (m *FunctionHalsteadMetrics) setVocabulary(v int)          { m.Vocabulary = v }
func (m *FunctionHalsteadMetrics) setLength(v int)              { m.Length = v }
func (m *FunctionHalsteadMetrics) setEstimatedLength(v float64) { m.EstimatedLength = v }
func (m *FunctionHalsteadMetrics) setVolume(v float64)          { m.Volume = v }
func (m *FunctionHalsteadMetrics) setDifficulty(v float64)      { m.Difficulty = v }
func (m *FunctionHalsteadMetrics) setEffort(v float64)          { m.Effort = v }
func (m *FunctionHalsteadMetrics) setTimeToProgram(v float64)   { m.TimeToProgram = v }
func (m *FunctionHalsteadMetrics) setDeliveredBugs(v float64)   { m.DeliveredBugs = v }

// SumMap sums all values in an integer map.
func (c *MetricsCalculator) SumMap(intMap map[string]int) int {
	total := 0
	for _, v := range intMap {
		total += v
	}

	return total
}

// CalculateHalsteadMetrics calculates all derived Halstead metrics.
// Works with both Metrics and FunctionHalsteadMetrics types.
func (c *MetricsCalculator) CalculateHalsteadMetrics(metrics halsteadMetrics) {
	distinctOps := metrics.getDistinctOperators()
	distinctOpnds := metrics.getDistinctOperands()
	totalOps := metrics.getTotalOperators()
	totalOpnds := metrics.getTotalOperands()

	vocabulary := distinctOps + distinctOpnds
	length := totalOps + totalOpnds

	metrics.setVocabulary(vocabulary)
	metrics.setLength(length)

	var estimatedLength float64

	if distinctOps > 0 && distinctOpnds > 0 {
		estimatedLength = float64(distinctOps)*math.Log2(float64(distinctOps)) +
			float64(distinctOpnds)*math.Log2(float64(distinctOpnds))
	}

	metrics.setEstimatedLength(estimatedLength)

	var volume float64

	if vocabulary > 0 {
		volume = float64(length) * math.Log2(float64(vocabulary))
	}

	metrics.setVolume(volume)

	var difficulty float64

	if distinctOpnds > 0 {
		difficulty = (float64(distinctOps) / DifficultyDivisor) * (float64(totalOpnds) / float64(distinctOpnds))
	}

	metrics.setDifficulty(difficulty)

	effort := volume * difficulty

	metrics.setEffort(effort)

	timeToProgram := effort / TimeConstant

	metrics.setTimeToProgram(timeToProgram)

	var deliveredBugs float64

	if effort > 0 {
		deliveredBugs = math.Pow(effort, BugExponent) / BugConstant
	}

	metrics.setDeliveredBugs(deliveredBugs)
}

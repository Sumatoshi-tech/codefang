package stats

// EMA computes an exponential moving average with a fixed smoothing factor.
type EMA struct {
	alpha       float64
	value       float64
	initialized bool
}

// NewEMA creates an EMA with the given smoothing factor alpha in (0, 1].
func NewEMA(alpha float64) *EMA {
	return &EMA{alpha: alpha}
}

// Update feeds a new observation and returns the updated average.
// The first call initializes the EMA to the observation value.
func (e *EMA) Update(v float64) float64 {
	if !e.initialized {
		e.value = v
		e.initialized = true

		return e.value
	}

	e.value = e.alpha*v + (1-e.alpha)*e.value

	return e.value
}

// Value returns the current EMA value (0 before any Update).
func (e *EMA) Value() float64 {
	return e.value
}

// Initialized reports whether Update has been called at least once.
func (e *EMA) Initialized() bool {
	return e.initialized
}

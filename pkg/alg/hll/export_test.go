package hll

// Precision returns the configured precision of the sketch.
func (s *Sketch) Precision() uint8 {
	return s.precision
}

// RegisterCount returns the number of registers (2^p).
func (s *Sketch) RegisterCount() uint {
	return uint(1) << s.precision
}

// Merge combines another sketch into this one by taking the element-wise
// maximum of registers. Both sketches must have the same precision.
func (s *Sketch) Merge(other *Sketch) error {
	if s.precision != other.precision {
		return ErrPrecisionMismatch
	}

	other.mu.RLock()
	defer other.mu.RUnlock()

	s.mu.Lock()
	defer s.mu.Unlock()

	for i, val := range other.registers {
		if val > s.registers[i] {
			s.registers[i] = val
		}
	}

	return nil
}

// Reset clears all registers without reallocating the underlying array.
func (s *Sketch) Reset() {
	s.mu.Lock()

	for i := range s.registers {
		s.registers[i] = 0
	}

	s.mu.Unlock()
}

// Clone creates a deep copy of the sketch.
func (s *Sketch) Clone() *Sketch {
	s.mu.RLock()
	defer s.mu.RUnlock()

	regs := make([]uint8, len(s.registers))
	copy(regs, s.registers)

	return &Sketch{
		registers: regs,
		precision: s.precision,
	}
}

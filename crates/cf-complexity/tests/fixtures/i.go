package main

// cyclomatic 2, cognitive 1, nesting 1
func initIf(m map[string]int) int {
	if v, ok := m["a"]; ok {
		return v
	}
	return 0
}

// cyclomatic 3, cognitive 3 (if +1, inner if nested +2), nesting 2
func initIfNested(m map[string]int) int {
	if v, ok := m["a"]; ok {
		if v > 1 {
			return v
		}
	}
	return 0
}

// cyclomatic 2, cognitive 1
func errIf() error {
	if err := doIt(); err != nil {
		return err
	}
	return nil
}

func doIt() error { return nil }

package main

// cyclomatic 4: 1 + case defaultValue + case 2 + case 3 (default excluded)
func caseNamedDefault(x, defaultValue int) int {
	switch x {
	case defaultValue:
		return 1
	case 2:
		return 2
	case 3:
		return 3
	default:
		return 0
	}
}

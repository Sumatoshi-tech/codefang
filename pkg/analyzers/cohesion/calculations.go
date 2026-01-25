package cohesion

import (
	"math"
	"slices"
)

// calculateLCOM calculates the Lack of Cohesion of Methods
func (c *CohesionAnalyzer) calculateLCOM(functions []Function) float64 {
	if len(functions) <= 1 {
		return 0.0
	}

	pairs := c.calculateFunctionPairs(functions)
	return c.computeLCOMScore(pairs)
}

// calculateFunctionPairs calculates all function pairs and their shared variable status
func (c *CohesionAnalyzer) calculateFunctionPairs(functions []Function) functionPairs {
	pairs := functionPairs{
		shared: 0,
		total:  0,
	}

	for i := range functions {
		for j := i + 1; j < len(functions); j++ {
			pairs.total++
			if c.haveSharedVariables(functions[i], functions[j]) {
				pairs.shared++
			}
		}
	}

	return pairs
}

// functionPairs holds the count of shared and total function pairs
type functionPairs struct {
	shared int
	total  int
}

// computeLCOMScore computes the final LCOM score from function pairs
func (c *CohesionAnalyzer) computeLCOMScore(pairs functionPairs) float64 {
	if pairs.total == 0 {
		return 0.0
	}

	return float64(pairs.total-pairs.shared) - float64(pairs.shared)
}

// haveSharedVariables checks if two functions share any variables
func (c *CohesionAnalyzer) haveSharedVariables(fn1, fn2 Function) bool {
	for _, var1 := range fn1.Variables {
		if c.containsVariable(fn2.Variables, var1) {
			return true
		}
	}
	return false
}

// containsVariable checks if a variable exists in the variables slice
func (c *CohesionAnalyzer) containsVariable(variables []string, target string) bool {
	return slices.Contains(variables, target)
}

// calculateCohesionScore calculates a normalized cohesion score (0-1)
func (c *CohesionAnalyzer) calculateCohesionScore(lcom float64, functionCount int) float64 {
	if functionCount <= 1 {
		return 1.0
	}

	maxLCOM := c.calculateMaxLCOM(functionCount)
	if maxLCOM == 0 {
		return 1.0
	}

	normalizedLCOM := lcom / maxLCOM
	cohesionScore := 1.0 - normalizedLCOM

	return c.clampCohesionScore(cohesionScore)
}

// calculateMaxLCOM calculates the maximum possible LCOM value
func (c *CohesionAnalyzer) calculateMaxLCOM(functionCount int) float64 {
	return float64(functionCount * (functionCount - 1) / 2)
}

// clampCohesionScore ensures the cohesion score is within valid bounds
func (c *CohesionAnalyzer) clampCohesionScore(score float64) float64 {
	return math.Max(0.0, math.Min(1.0, score))
}

// calculateFunctionCohesion calculates average function-level cohesion
func (c *CohesionAnalyzer) calculateFunctionCohesion(functions []Function) float64 {
	if len(functions) == 0 {
		return 1.0
	}

	total := c.sumFunctionCohesion(functions)
	return total / float64(len(functions))
}

// sumFunctionCohesion calculates the sum of all function cohesion values
func (c *CohesionAnalyzer) sumFunctionCohesion(functions []Function) float64 {
	total := 0.0
	for _, fn := range functions {
		total += fn.Cohesion
	}
	return total
}

// calculateFunctionLevelCohesion calculates cohesion for a single function using an improved
// algorithm that doesn't penalize small, focused functions like getters, setters, or simple
// utility functions (e.g., Register functions).
//
// The algorithm uses a tiered approach:
// 1. Small functions (≤5 lines) with ≤3 variables: Perfect cohesion (1.0)
//   - These are typically focused, single-purpose functions
//   - Examples: getters, setters, simple validators, registration functions
//
// 2. Small functions with more variables: Gentle linear penalty
//   - Still considers them relatively cohesive but applies small penalty
//
// 3. Larger functions: Logarithmic scaling to avoid harsh penalties
//   - Uses variable density with logarithmic penalty curve
//   - More forgiving than linear scaling for moderately complex functions
func (c *CohesionAnalyzer) calculateFunctionLevelCohesion(fn Function) float64 {
	if fn.LineCount == 0 {
		return 1.0
	}

	if c.isSmallFunction(fn) {
		return c.calculateSmallFunctionCohesion(fn)
	}

	return c.calculateLargeFunctionCohesion(fn)
}

// isSmallFunction checks if a function is small (≤5 lines)
func (c *CohesionAnalyzer) isSmallFunction(fn Function) bool {
	return fn.LineCount <= 5
}

// calculateSmallFunctionCohesion calculates cohesion for small functions
func (c *CohesionAnalyzer) calculateSmallFunctionCohesion(fn Function) float64 {
	if c.hasFewVariables(fn) {
		return 1.0
	}
	return c.calculateSmallFunctionPenalty(fn)
}

// hasFewVariables checks if a function has 3 or fewer variables
func (c *CohesionAnalyzer) hasFewVariables(fn Function) bool {
	return len(fn.Variables) <= 3
}

// calculateSmallFunctionPenalty calculates the penalty for small functions with many variables
func (c *CohesionAnalyzer) calculateSmallFunctionPenalty(fn Function) float64 {
	penalty := float64(len(fn.Variables)-3) * 0.1
	return math.Max(0.7, 1.0-penalty)
}

// calculateLargeFunctionCohesion calculates cohesion for larger functions
func (c *CohesionAnalyzer) calculateLargeFunctionCohesion(fn Function) float64 {
	variableDensity := c.calculateVariableDensity(fn)

	if c.hasLowVariableDensity(variableDensity) {
		return c.calculateLowDensityCohesion(variableDensity)
	}

	return c.calculateHighDensityCohesion(variableDensity)
}

// calculateVariableDensity calculates the ratio of variables to lines of code
func (c *CohesionAnalyzer) calculateVariableDensity(fn Function) float64 {
	return float64(len(fn.Variables)) / float64(fn.LineCount)
}

// hasLowVariableDensity checks if the variable density is low (≤0.5)
func (c *CohesionAnalyzer) hasLowVariableDensity(density float64) bool {
	return density <= 0.5
}

// calculateLowDensityCohesion calculates cohesion for functions with low variable density
func (c *CohesionAnalyzer) calculateLowDensityCohesion(density float64) float64 {
	return 1.0 - density
}

// calculateHighDensityCohesion calculates cohesion for functions with high variable density
func (c *CohesionAnalyzer) calculateHighDensityCohesion(density float64) float64 {
	penalty := c.calculateLogarithmicPenalty(density)
	cohesion := 1.0 - math.Min(0.8, penalty)
	return math.Max(0.2, cohesion)
}

// calculateLogarithmicPenalty calculates the logarithmic penalty for high variable density
func (c *CohesionAnalyzer) calculateLogarithmicPenalty(density float64) float64 {
	return math.Log2(1.0+density) / 2.0
}

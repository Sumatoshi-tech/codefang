package cohesion

import (
	"math"
	"slices"
)

// calculateLCOM calculates the Lack of Cohesion of Methods using the Henderson-Sellers
// formula (LCOM-HS), the industry standard used by NDepend, JArchitect, and CppDepend.
//
// Formula: LCOM = 1 - sum(mA) / (m * a)
//   - m = number of functions
//   - a = number of distinct variables across all functions
//   - mA = for each variable, count of functions that use it
//
// Range: [0, 1] where 0 = perfect cohesion (all functions use all variables),
// 1 = no cohesion (each variable used by only one function).
func (c *Analyzer) calculateLCOM(functions []Function) float64 {
	if len(functions) <= 1 {
		return 0.0
	}

	allVars := collectUniqueVariables(functions)
	if len(allVars) == 0 {
		return 0.0
	}

	m := float64(len(functions))
	a := float64(len(allVars))

	sumMA := countVariableAccesses(allVars, functions)

	return clamp01(1.0 - (sumMA / (m * a)))
}

// collectUniqueVariables gathers all distinct variable names across all functions.
func collectUniqueVariables(functions []Function) []string {
	seen := make(map[string]struct{})

	for i := range functions {
		for _, v := range functions[i].Variables {
			seen[v] = struct{}{}
		}
	}

	result := make([]string, 0, len(seen))
	for v := range seen {
		result = append(result, v)
	}

	return result
}

// countVariableAccesses counts the total number of function-variable access pairs.
// For each variable, it counts how many functions reference it.
func countVariableAccesses(allVars []string, functions []Function) float64 {
	sum := 0.0

	for _, varName := range allVars {
		for i := range functions {
			if slices.Contains(functions[i].Variables, varName) {
				sum++
			}
		}
	}

	return sum
}

// calculateCohesionScore converts LCOM-HS to a cohesion score where higher is better.
// Since LCOM-HS is already normalized to [0,1], cohesion = 1 - LCOM.
func (c *Analyzer) calculateCohesionScore(lcom float64, functionCount int) float64 {
	if functionCount <= 1 {
		return 1.0
	}

	return clamp01(1.0 - lcom)
}

// calculateFunctionCohesion calculates average function-level cohesion.
func (c *Analyzer) calculateFunctionCohesion(functions []Function) float64 {
	if len(functions) == 0 {
		return 1.0
	}

	total := 0.0
	for _, fn := range functions {
		total += fn.Cohesion
	}

	return total / float64(len(functions))
}

// calculateFunctionLevelCohesion computes cohesion for a single function based on
// what fraction of its variables are shared with at least one other function.
// This measures communicational cohesion (Yourdon-Constantine level 5).
//
// Formula: cohesion = sharedVars / totalUniqueVars
//   - sharedVars = variables that appear in at least one other function
//   - totalUniqueVars = distinct variables in this function
//
// Range: [0, 1] where 1 = all variables are shared (high cohesion with the module),
// 0 = no variables are shared (isolated function).
// Functions with no variables get a score of 1.0 (trivial, no penalty).
func (c *Analyzer) calculateFunctionLevelCohesion(fn Function, allFunctions []Function) float64 {
	uniqueVars := uniqueStrings(fn.Variables)
	if len(uniqueVars) == 0 {
		return 1.0
	}

	otherVars := collectOtherFunctionVariables(fn.Name, allFunctions)
	if len(otherVars) == 0 {
		return 0.0
	}

	shared := 0

	for _, v := range uniqueVars {
		if _, exists := otherVars[v]; exists {
			shared++
		}
	}

	return float64(shared) / float64(len(uniqueVars))
}

// collectOtherFunctionVariables gathers all variable names from functions other than
// the specified one, returning them as a set for O(1) lookup.
func collectOtherFunctionVariables(excludeName string, functions []Function) map[string]struct{} {
	result := make(map[string]struct{})

	for i := range functions {
		if functions[i].Name == excludeName {
			continue
		}

		for _, v := range functions[i].Variables {
			result[v] = struct{}{}
		}
	}

	return result
}

// uniqueStrings returns a deduplicated copy of a string slice.
func uniqueStrings(ss []string) []string {
	seen := make(map[string]struct{}, len(ss))
	result := make([]string, 0, len(ss))

	for _, s := range ss {
		if _, exists := seen[s]; !exists {
			seen[s] = struct{}{}
			result = append(result, s)
		}
	}

	return result
}

// clamp01 clamps a value to [0, 1].
func clamp01(v float64) float64 {
	return math.Max(0.0, math.Min(1.0, v))
}

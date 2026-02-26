package cohesion

import (
	"math"

	"github.com/Sumatoshi-tech/codefang/pkg/alg/bloom"
)

// Bloom filter configuration for per-function variable sets.
const (
	bloomFPRate         = 0.01 // 1% false-positive rate.
	bloomMinElements    = 16   // Minimum expected elements to avoid degenerate filters.
	bloomGlobalMinElems = 64   // Minimum for the global variable filter.
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
//
// Uses Bloom filters per function for O(1) membership tests instead of
// O(M) linear scans via [slices.Contains].
func countVariableAccesses(allVars []string, functions []Function) float64 {
	filters := buildPerFunctionBloomFilters(functions)

	sum := 0.0

	for _, varName := range allVars {
		key := []byte(varName)
		for i := range filters {
			if filters[i] != nil && filters[i].Test(key) {
				sum++
			}
		}
	}

	return sum
}

// buildPerFunctionBloomFilters creates a Bloom filter for each function's variable set.
func buildPerFunctionBloomFilters(functions []Function) []*bloom.Filter {
	filters := make([]*bloom.Filter, len(functions))

	for i := range functions {
		vars := functions[i].Variables
		n := max(uint(len(vars)), bloomMinElements)

		f, err := bloom.NewWithEstimates(n, bloomFPRate)
		if err != nil {
			continue
		}

		for _, v := range vars {
			f.Add([]byte(v))
		}

		filters[i] = f
	}

	return filters
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
//
// The otherVarsFilter is a pre-built Bloom filter containing variables from all
// OTHER functions (excluding this one's unique contributions). Because the filter
// is built from the global set minus nothing (all functions contribute), a variable
// found in the filter is shared unless it is unique to this function alone. The
// false-positive rate is bounded by bloomFPRate.
func (c *Analyzer) calculateFunctionLevelCohesion(fn Function, globalFilter *bloom.Filter) float64 {
	uniqueVars := uniqueStrings(fn.Variables)
	if len(uniqueVars) == 0 {
		return 1.0
	}

	if globalFilter == nil {
		return 0.0
	}

	shared := 0

	for _, v := range uniqueVars {
		if globalFilter.Test([]byte(v)) {
			shared++
		}
	}

	return float64(shared) / float64(len(uniqueVars))
}

// buildGlobalVariableFilter creates a Bloom filter containing all variables from
// all functions. Used to quickly test whether a variable is shared with at least
// one other function. For a given function, any variable that appears in the global
// filter AND has count > 1 across all functions is shared. We use the simpler
// approximation: if a variable appears in the global filter it is likely shared,
// since the filter contains variables from ALL functions.
func buildGlobalVariableFilter(functions []Function) *bloom.Filter {
	// Count total unique variables for sizing.
	seen := make(map[string]int)

	for i := range functions {
		for _, v := range functions[i].Variables {
			seen[v]++
		}
	}

	if len(seen) == 0 {
		return nil
	}

	// Only include variables that appear in more than one function (truly shared).
	n := uint(0)

	for _, count := range seen {
		if count > 1 {
			n++
		}
	}

	if n == 0 {
		return nil
	}

	n = max(n, bloomGlobalMinElems)

	f, err := bloom.NewWithEstimates(n, bloomFPRate)
	if err != nil {
		return nil
	}

	for v, count := range seen {
		if count > 1 {
			f.Add([]byte(v))
		}
	}

	return f
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

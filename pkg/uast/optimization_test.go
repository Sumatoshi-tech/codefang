package uast //nolint:testpackage // White-box tests access internal optimization functions.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/mapping"
)

//nolint:gocognit,nestif // Test function exercises multiple matcher interface methods.
func TestPatternMatcherOptimizations(t *testing.T) {
	// Test that pattern matchers are generated correctly.
	goMatcher := GetPatternMatcher("go")
	if goMatcher == nil {
		t.Fatal("Go pattern matcher should be available")
	}

	// Test type assertion - use interface{} since we don't know exact type.
	if matcher, ok := goMatcher.(interface {
		MatchPattern(string) (mapping.Rule, bool)
		GetRulesCount() int
		GetRuleByIndex(int) (mapping.Rule, bool)
		GetRuleIndex(string) (int, bool)
	}); ok {
		// Test pattern matching.
		rule, exists := matcher.MatchPattern("function_declaration")
		if !exists {
			t.Error("function_declaration pattern should exist")
		}

		if rule.Name != "function_declaration" {
			t.Errorf("Expected function_declaration, got %s", rule.Name)
		}

		// Test rule count.
		count := matcher.GetRulesCount()
		if count == 0 {
			t.Error("Should have rules")
		}

		// Test rule by index.
		ruleByIndex, exists := matcher.GetRuleByIndex(0)
		if !exists {
			t.Error("Should be able to get rule by index")
		}

		if ruleByIndex.Name == "" {
			t.Error("Rule should have a name")
		}

		// Test rule index lookup.
		index, exists := matcher.GetRuleIndex("function_declaration")
		if !exists {
			t.Error("Should find rule index")
		}

		if index < 0 {
			t.Error("Index should be non-negative")
		}
	} else {
		t.Fatal("Go pattern matcher should be of correct type")
	}
}

func TestValidationFunctions(t *testing.T) {
	// Test that validation functions are generated.
	err := validategoRules()
	if err != nil {
		t.Errorf("Go rules validation failed: %v", err)
	}

	// Test other languages.
	err = validateyamlRules()
	if err != nil {
		t.Errorf("YAML rules validation failed: %v", err)
	}
}

func TestPerformanceMetrics(t *testing.T) {
	// Test metrics recording.
	RecordPatternMatch("go", "function_declaration", true)
	RecordPatternMatch("go", "if_statement", false)

	// Test metrics retrieval.
	stats := GetPatternMatchStats()
	if len(stats) == 0 {
		t.Error("Should have recorded metrics")
	}

	// Check for expected metrics.
	expectedKeys := []string{
		"go:function_declaration_matches",
		"go:if_statement_misses",
	}

	for _, key := range expectedKeys {
		if stats[key] == 0 {
			t.Errorf("Expected metric %s to be recorded", key)
		}
	}
}

func TestPatternMatcherRegistry(t *testing.T) {
	// Test that all languages have pattern matchers.
	expectedLanguages := []string{"go", "yaml", "javascript", "python"}

	for _, lang := range expectedLanguages {
		matcher := GetPatternMatcher(lang)
		if matcher == nil {
			t.Errorf("Pattern matcher for %s should be available", lang)
		}
	}
}

func BenchmarkPatternMatching(b *testing.B) {
	goMatcher := GetPatternMatcher("go")

	b.ResetTimer()

	for b.Loop() {
		// Test O(1) pattern lookup.
		if matcher, ok := goMatcher.(interface {
			MatchPattern(string) (mapping.Rule, bool)
		}); ok {
			rule, exists := matcher.MatchPattern("function_declaration")
			if !exists {
				b.Fatal("Pattern should exist")
			}

			if rule.Name != "function_declaration" {
				b.Fatal("Wrong rule returned")
			}
		}
	}
}

func BenchmarkRuleByIndex(b *testing.B) {
	goMatcher := GetPatternMatcher("go")

	b.ResetTimer()

	for b.Loop() {
		// Test O(1) index lookup.
		if matcher, ok := goMatcher.(interface {
			GetRuleByIndex(int) (mapping.Rule, bool)
			GetRulesCount() int
		}); ok {
			rule, exists := matcher.GetRuleByIndex(0)
			if !exists {
				b.Fatal("Rule should exist")
			}

			if rule.Name == "" {
				b.Fatal("Rule should have name")
			}
		}
	}
}

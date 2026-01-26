package common //nolint:testpackage // testing internal implementation.

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestNewDataCollector(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")
	if collector == nil {
		t.Fatal("NewDataCollector returned nil")
	}

	if collector.collectionKey != "items" {
		t.Errorf("expected collectionKey 'items', got '%s'", collector.collectionKey)
	}

	if collector.identifierKey != "name" {
		t.Errorf("expected identifierKey 'name', got '%s'", collector.identifierKey)
	}
}

func TestDataCollector_CollectFromReport(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")

	report := analyze.Report{
		"items": []map[string]any{
			{"name": "item1", "value": 100},
			{"name": "item2", "value": 200},
		},
	}

	collector.CollectFromReport(report)

	if collector.GetDataCount() != 2 {
		t.Errorf("expected 2 items, got %d", collector.GetDataCount())
	}
}

func TestDataCollector_CollectFromReport_InvalidCollection(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")

	// Test with wrong type for collection.
	report := analyze.Report{
		"items": "not a slice",
	}

	collector.CollectFromReport(report)

	if collector.GetDataCount() != 0 {
		t.Errorf("expected 0 items for invalid collection, got %d", collector.GetDataCount())
	}
}

func TestDataCollector_CollectFromReport_MissingIdentifier(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")

	// Test with items missing the identifier key.
	report := analyze.Report{
		"items": []map[string]any{
			{"id": "item1", "value": 100}, // Using 'id' instead of 'name'.
		},
	}

	collector.CollectFromReport(report)

	if collector.GetDataCount() != 0 {
		t.Errorf("expected 0 items when identifier is missing, got %d", collector.GetDataCount())
	}
}

func TestDataCollector_GetSortedData(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")

	report := analyze.Report{
		"items": []map[string]any{
			{"name": "charlie", "value": 300},
			{"name": "alpha", "value": 100},
			{"name": "bravo", "value": 200},
		},
	}

	collector.CollectFromReport(report)
	sortedData := collector.GetSortedData()

	if len(sortedData) != 3 {
		t.Fatalf("expected 3 items, got %d", len(sortedData))
	}

	// Verify sorted order.
	expectedOrder := []string{"alpha", "bravo", "charlie"}
	for i, expected := range expectedOrder {
		if sortedData[i]["name"] != expected {
			t.Errorf("expected '%s' at index %d, got '%v'", expected, i, sortedData[i]["name"])
		}
	}
}

func TestDataCollector_GetSortedData_Empty(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")
	sortedData := collector.GetSortedData()

	if len(sortedData) != 0 {
		t.Errorf("expected empty slice, got %d items", len(sortedData))
	}
}

func TestDataCollector_GetDataCount(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")

	// Initially empty.
	if collector.GetDataCount() != 0 {
		t.Error("expected 0 initial count")
	}

	// Add some data.
	report := analyze.Report{
		"items": []map[string]any{
			{"name": "item1"},
			{"name": "item2"},
		},
	}
	collector.CollectFromReport(report)

	if collector.GetDataCount() != 2 {
		t.Errorf("expected 2, got %d", collector.GetDataCount())
	}
}

func TestDataCollector_GetCollectionKey(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("functions", "func_name")
	if collector.GetCollectionKey() != "functions" {
		t.Errorf("expected 'functions', got '%s'", collector.GetCollectionKey())
	}
}

func TestDataCollector_GetIdentifierKey(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("functions", "func_name")
	if collector.GetIdentifierKey() != "func_name" {
		t.Errorf("expected 'func_name', got '%s'", collector.GetIdentifierKey())
	}
}

func TestDataCollector_Deduplication(t *testing.T) {
	t.Parallel()

	collector := NewDataCollector("items", "name")

	// Collect same item twice (should deduplicate).
	report1 := analyze.Report{
		"items": []map[string]any{
			{"name": "item1", "value": 100},
		},
	}
	report2 := analyze.Report{
		"items": []map[string]any{
			{"name": "item1", "value": 200}, // Same name, different value.
		},
	}

	collector.CollectFromReport(report1)
	collector.CollectFromReport(report2)

	// Should only have 1 item (last one wins).
	if collector.GetDataCount() != 1 {
		t.Errorf("expected 1 item after deduplication, got %d", collector.GetDataCount())
	}

	sortedData := collector.GetSortedData()
	if sortedData[0]["value"] != 200 {
		t.Error("expected last value to be kept")
	}
}

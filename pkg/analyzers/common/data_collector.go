package common

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// DataCollector manages the collection and organization of data from reports
type DataCollector struct {
	collectionKey string
	identifierKey string
	collectedData map[string]interface{}
}

// NewDataCollector creates a new DataCollector
func NewDataCollector(collectionKey, identifierKey string) *DataCollector {
	return &DataCollector{
		collectionKey: collectionKey,
		identifierKey: identifierKey,
		collectedData: make(map[string]interface{}),
	}
}

// CollectFromReport extracts data from a single report
func (dc *DataCollector) CollectFromReport(report analyze.Report) {
	if collection, ok := report[dc.collectionKey].([]map[string]interface{}); ok {
		for _, item := range collection {
			if identifier, ok := item[dc.identifierKey].(string); ok {
				dc.collectedData[identifier] = item
			}
		}
	}
}

// GetSortedData returns the collected data in sorted order
func (dc *DataCollector) GetSortedData() []map[string]interface{} {
	data := make([]map[string]interface{}, 0, len(dc.collectedData))

	for _, item := range dc.collectedData {
		if itemMap, ok := item.(map[string]interface{}); ok {
			data = append(data, itemMap)
		}
	}

	// Sort by identifier
	sort.Slice(data, func(i, j int) bool {
		identifierI, _ := data[i][dc.identifierKey].(string)
		identifierJ, _ := data[j][dc.identifierKey].(string)
		return identifierI < identifierJ
	})

	return data
}

// GetDataCount returns the number of collected items
func (dc *DataCollector) GetDataCount() int {
	return len(dc.collectedData)
}

// GetCollectionKey returns the collection key
func (dc *DataCollector) GetCollectionKey() string {
	return dc.collectionKey
}

// GetIdentifierKey returns the identifier key
func (dc *DataCollector) GetIdentifierKey() string {
	return dc.identifierKey
}

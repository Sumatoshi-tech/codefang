package common

import (
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// DataCollector manages the collection and organization of data from reports.
type DataCollector struct {
	collectedData map[string]any
	collectionKey string
	identifierKey string
}

// NewDataCollector creates a new DataCollector.
func NewDataCollector(collectionKey, identifierKey string) *DataCollector {
	return &DataCollector{
		collectionKey: collectionKey,
		identifierKey: identifierKey,
		collectedData: make(map[string]any),
	}
}

// CollectFromReport extracts data from a single report.
func (dc *DataCollector) CollectFromReport(report analyze.Report) {
	if collection, ok := report[dc.collectionKey].([]map[string]any); ok {
		for _, item := range collection {
			if identifier, idOK := item[dc.identifierKey].(string); idOK {
				dc.collectedData[identifier] = item
			}
		}
	}
}

// GetSortedData returns the collected data in sorted order.
func (dc *DataCollector) GetSortedData() []map[string]any {
	data := make([]map[string]any, 0, len(dc.collectedData))

	for _, item := range dc.collectedData {
		if itemMap, ok := item.(map[string]any); ok {
			data = append(data, itemMap)
		}
	}

	// Sort by identifier.
	sort.Slice(data, func(i, j int) bool {
		identifierI, iOK := data[i][dc.identifierKey].(string)
		if !iOK {
			identifierI = ""
		}

		identifierJ, jOK := data[j][dc.identifierKey].(string)
		if !jOK {
			identifierJ = ""
		}

		return identifierI < identifierJ
	})

	return data
}

// GetDataCount returns the number of collected items.
func (dc *DataCollector) GetDataCount() int {
	return len(dc.collectedData)
}

// GetCollectionKey returns the collection key.
func (dc *DataCollector) GetCollectionKey() string {
	return dc.collectionKey
}

// GetIdentifierKey returns the identifier key.
func (dc *DataCollector) GetIdentifierKey() string {
	return dc.identifierKey
}

package devs

import (
	"fmt"
	"slices"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed devs data from a ReportReader
// and builds the same dashboard sections as GenerateSections, without
// materializing a full Report or recomputing metrics.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	developers, devErr := readDevelopersIfPresent(reader, kinds)
	if devErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindDeveloper, devErr)
	}

	languages, langErr := readLanguagesIfPresent(reader, kinds)
	if langErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindLanguage, langErr)
	}

	busFactor, bfErr := readBusFactorIfPresent(reader, kinds)
	if bfErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindBusFactor, bfErr)
	}

	activity, actErr := readActivityIfPresent(reader, kinds)
	if actErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindActivity, actErr)
	}

	churn, churnErr := readChurnIfPresent(reader, kinds)
	if churnErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindChurn, churnErr)
	}

	agg, aggErr := readAggregateIfPresent(reader, kinds)
	if aggErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindAggregate, aggErr)
	}

	if len(developers) == 0 && len(activity) == 0 {
		return nil, nil
	}

	metrics := &ComputedMetrics{
		Developers: developers,
		Languages:  languages,
		BusFactor:  busFactor,
		Activity:   activity,
		Churn:      churn,
		Aggregate:  agg,
	}

	topLangs := make([]string, 0, topLanguagesForRadar)

	for i, ld := range languages {
		if i >= topLanguagesForRadar {
			break
		}

		topLangs = append(topLangs, ld.Name)
	}

	data := &DashboardData{
		Metrics:      metrics,
		TopLanguages: topLangs,
	}

	tabs := createDashboardTabs(data)

	return []plotpage.Section{
		{
			Title:    "Developer Analytics",
			Subtitle: "Multi-dimensional view of team contributions and codebase ownership",
			Chart:    tabs,
		},
	}, nil
}

// readDevelopersIfPresent reads all developer records, returning nil if absent.
func readDevelopersIfPresent(reader analyze.ReportReader, kinds []string) ([]DeveloperData, error) {
	if !slices.Contains(kinds, KindDeveloper) {
		return nil, nil
	}

	var result []DeveloperData

	iterErr := reader.Iter(KindDeveloper, func(raw []byte) error {
		var record DeveloperData

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// readLanguagesIfPresent reads all language records, returning nil if absent.
func readLanguagesIfPresent(reader analyze.ReportReader, kinds []string) ([]LanguageData, error) {
	if !slices.Contains(kinds, KindLanguage) {
		return nil, nil
	}

	var result []LanguageData

	iterErr := reader.Iter(KindLanguage, func(raw []byte) error {
		var record LanguageData

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// readBusFactorIfPresent reads all bus factor records, returning nil if absent.
func readBusFactorIfPresent(reader analyze.ReportReader, kinds []string) ([]BusFactorData, error) {
	if !slices.Contains(kinds, KindBusFactor) {
		return nil, nil
	}

	var result []BusFactorData

	iterErr := reader.Iter(KindBusFactor, func(raw []byte) error {
		var record BusFactorData

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// readActivityIfPresent reads all activity records, returning nil if absent.
func readActivityIfPresent(reader analyze.ReportReader, kinds []string) ([]ActivityData, error) {
	if !slices.Contains(kinds, KindActivity) {
		return nil, nil
	}

	var result []ActivityData

	iterErr := reader.Iter(KindActivity, func(raw []byte) error {
		var record ActivityData

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// readChurnIfPresent reads all churn records, returning nil if absent.
func readChurnIfPresent(reader analyze.ReportReader, kinds []string) ([]ChurnData, error) {
	if !slices.Contains(kinds, KindChurn) {
		return nil, nil
	}

	var result []ChurnData

	iterErr := reader.Iter(KindChurn, func(raw []byte) error {
		var record ChurnData

		decErr := analyze.GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// readAggregateIfPresent reads the single aggregate record, returning zero value if absent.
func readAggregateIfPresent(reader analyze.ReportReader, kinds []string) (AggregateData, error) {
	if !slices.Contains(kinds, KindAggregate) {
		return AggregateData{}, nil
	}

	var agg AggregateData

	iterErr := reader.Iter(KindAggregate, func(raw []byte) error {
		return analyze.GobDecode(raw, &agg)
	})

	return agg, iterErr
}

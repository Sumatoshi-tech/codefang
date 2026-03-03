package couples

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/plotpage"
)

// GenerateStoreSections reads pre-computed coupling data from a ReportReader
// and builds the same plot sections as GenerateSections, without materializing
// a full Report or dense O(N²) matrix.
func GenerateStoreSections(reader analyze.ReportReader) ([]plotpage.Section, error) {
	kinds := reader.Kinds()

	fileCoupling, fcErr := readFileCouplingIfPresent(reader, kinds)
	if fcErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindFileCoupling, fcErr)
	}

	devMatrix, dmErr := readDevMatrixIfPresent(reader, kinds)
	if dmErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindDevMatrix, dmErr)
	}

	ownership, owErr := readOwnershipIfPresent(reader, kinds)
	if owErr != nil {
		return nil, fmt.Errorf("read %s: %w", KindOwnership, owErr)
	}

	return buildStoreSections(fileCoupling, devMatrix, ownership)
}

// readFileCouplingIfPresent reads all "file_coupling" records, returning nil if the kind is absent.
func readFileCouplingIfPresent(reader analyze.ReportReader, kinds []string) ([]FileCouplingData, error) {
	return analyze.ReadRecordsIfPresent[FileCouplingData](reader, kinds, KindFileCoupling)
}

// readDevMatrixIfPresent reads the "dev_matrix" record, returning zero value if the kind is absent.
func readDevMatrixIfPresent(reader analyze.ReportReader, kinds []string) (StoreDevMatrix, error) {
	return analyze.ReadRecordIfPresent[StoreDevMatrix](reader, kinds, KindDevMatrix)
}

// readOwnershipIfPresent reads all "ownership" records, returning nil if the kind is absent.
func readOwnershipIfPresent(reader analyze.ReportReader, kinds []string) ([]FileOwnershipData, error) {
	return analyze.ReadRecordsIfPresent[FileOwnershipData](reader, kinds, KindOwnership)
}

// buildStoreSections constructs the three couples plot sections from pre-computed data.
func buildStoreSections(
	fileCoupling []FileCouplingData,
	devMatrix StoreDevMatrix,
	ownership []FileOwnershipData,
) ([]plotpage.Section, error) {
	var result []plotpage.Section

	// Section 1: File coupling bar chart.
	fileCouplingChart := buildFileCouplingBarChartFromData(fileCoupling)
	if fileCouplingChart != nil {
		result = append(result, plotpage.Section{
			Title:    "Top File Couples",
			Subtitle: "Most frequently co-changed file pairs across commit history.",
			Chart:    plotpage.WrapChart(fileCouplingChart),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Tall bars = file pairs that frequently change together",
					"Cross-package coupling may indicate architectural issues",
					"Test files coupled with implementation is expected and healthy",
					"Action: Consider extracting shared logic or merging tightly coupled files",
				},
			},
		})
	}

	// Section 2: Developer coupling heatmap.
	if len(devMatrix.Names) > 0 {
		heatmap := buildHeatmapFromMatrix(devMatrix.Matrix, devMatrix.Names)

		result = append(result, plotpage.Section{
			Title:    "Developer Coupling Heatmap",
			Subtitle: "Shows how often developers work on the same files in the same commits.",
			Chart:    plotpage.WrapChart(heatmap),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"High values on diagonal = individual developer activity",
					"High off-diagonal values = developers frequently working on the same code",
					"Symmetric patterns = collaborative pairs who often commit together",
					"Look for: Isolated developers or tight clusters",
					"Action: High coupling may indicate knowledge sharing or ownership issues",
				},
			},
		})
	}

	// Section 3: Ownership distribution pie chart.
	ownershipPie := buildOwnershipPieChartFromData(ownership)
	if ownershipPie != nil {
		result = append(result, plotpage.Section{
			Title:    "File Ownership Distribution",
			Subtitle: "How files are distributed by number of contributors.",
			Chart:    plotpage.WrapChart(ownershipPie),
			Hint: plotpage.Hint{
				Title: "How to interpret:",
				Items: []string{
					"Single owner = bus factor risk if that person leaves",
					"Many owners = potential coordination overhead",
					"2-3 owners is often the healthy sweet spot",
					"Action: Review single-owner files for knowledge sharing opportunities",
				},
			},
		})
	}

	return result, nil
}

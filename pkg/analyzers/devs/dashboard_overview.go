package devs

import (
	"fmt"
	"io"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

type overviewContent struct {
	data *DashboardData
}

func createOverviewTab(data *DashboardData) *overviewContent {
	return &overviewContent{data: data}
}

// Render implements the Renderable interface for the overview tab.
func (o *overviewContent) Render(w io.Writer) error {
	err := o.renderStats(w)
	if err != nil {
		return err
	}

	err = o.renderContributorsTable(w)
	if err != nil {
		return err
	}

	return o.renderRiskAlert(w)
}

func (o *overviewContent) renderStats(w io.Writer) error {
	statsGrid := plotpage.NewGrid(statsGridCols,
		plotpage.NewStat("Total Commits", formatNumber(o.data.TotalCommits)),
		plotpage.NewStat("Total Developers", strconv.Itoa(o.data.TotalDevelopers)),
		plotpage.NewStat("Active Developers", strconv.Itoa(o.data.ActiveDevelopers)),
		plotpage.NewStat("Languages", strconv.Itoa(len(o.data.LanguageSummaries))),
	)

	return statsGrid.Render(w)
}

func (o *overviewContent) renderContributorsTable(w io.Writer) error {
	_, err := w.Write([]byte(`<div class="mt-6">`))
	if err != nil {
		return fmt.Errorf("writing table div: %w", err)
	}

	card := plotpage.NewCard("Top Contributors", "Developers ranked by total commits")
	table := plotpage.NewTable([]string{"Rank", "Developer", "Commits", "Lines Added", "Lines Removed", "Net Lines"})

	count := min(overviewTableLimit, len(o.data.DevSummaries))

	for i := range count {
		ds := o.data.DevSummaries[i]
		table.AddRow(
			strconv.Itoa(i+1),
			ds.Name,
			formatNumber(ds.Commits),
			formatNumber(ds.Added),
			formatNumber(ds.Removed),
			formatSignedNumber(ds.NetLines),
		)
	}

	card.WithContent(table)

	err = card.Render(w)
	if err != nil {
		return err
	}

	_, err = w.Write([]byte(`</div>`))
	if err != nil {
		return fmt.Errorf("writing closing div: %w", err)
	}

	return nil
}

func (o *overviewContent) renderRiskAlert(w io.Writer) error {
	critical, high := o.countHighRisks()

	if critical == 0 && high == 0 {
		return nil
	}

	_, err := w.Write([]byte(`<div class="mt-6">`))
	if err != nil {
		return fmt.Errorf("writing alert div: %w", err)
	}

	alertMsg := fmt.Sprintf(
		"%d languages have CRITICAL bus factor risk, %d have HIGH risk. See Bus Factor tab for details.",
		critical, high)
	alert := plotpage.NewAlert("Bus Factor Warning", alertMsg, plotpage.BadgeWarning)

	err = alert.Render(w)
	if err != nil {
		return err
	}

	_, err = w.Write([]byte(`</div>`))
	if err != nil {
		return fmt.Errorf("writing closing div: %w", err)
	}

	return nil
}

func (o *overviewContent) countHighRisks() (critical, high int) {
	for _, entry := range o.data.BusFactorEntries {
		switch entry.RiskLevel {
		case riskCritical:
			critical++
		case riskHigh:
			high++
		}
	}

	return critical, high
}

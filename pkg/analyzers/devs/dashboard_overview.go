package devs

import (
	"fmt"
	"io"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

const statsGridCols = 4

type overviewContent struct {
	data *DashboardData
}

func createOverviewTab(data *DashboardData) *overviewContent {
	return &overviewContent{data: data}
}

// Render renders the overview content to the writer.
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
	m := o.data.Metrics
	statsGrid := plotpage.NewGrid(statsGridCols,
		plotpage.NewStat("Total Commits", formatNumber(m.Aggregate.TotalCommits)),
		plotpage.NewStat("Total Developers", strconv.Itoa(m.Aggregate.TotalDevelopers)),
		plotpage.NewStat("Active Developers", strconv.Itoa(m.Aggregate.ActiveDevelopers)),
		plotpage.NewStat("Languages", strconv.Itoa(len(m.Languages))),
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

	count := min(overviewTableLimit, len(o.data.Metrics.Developers))
	for i := range count {
		dev := o.data.Metrics.Developers[i]
		table.AddRow(
			strconv.Itoa(i+1),
			dev.Name,
			formatNumber(dev.Commits),
			formatNumber(dev.Added),
			formatNumber(dev.Removed),
			formatSignedNumber(dev.NetLines),
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
	for _, bf := range o.data.Metrics.BusFactor {
		switch bf.RiskLevel {
		case RiskCritical:
			critical++
		case RiskHigh:
			high++
		}
	}

	return critical, high
}

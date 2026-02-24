package devs

import (
	"bytes"
	"fmt"
	"io"
	"strconv"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

type busfactorContent struct {
	data *DashboardData
}

func createBusFactorTab(data *DashboardData) *busfactorContent {
	return &busfactorContent{data: data}
}

// Render renders the bus factor content to the writer.
func (bf *busfactorContent) Render(w io.Writer) error {
	if len(bf.data.Metrics.BusFactor) == 0 {
		return plotpage.NewText("No bus factor data available").Render(w)
	}

	err := bf.renderSummary(w)
	if err != nil {
		return err
	}

	return bf.renderTable(w)
}

func (bf *busfactorContent) renderSummary(w io.Writer) error {
	critical, high, medium, low := bf.countByRiskLevel()

	grid := plotpage.NewGrid(statsGridCols,
		plotpage.NewStat("Critical Risk", strconv.Itoa(critical)).WithTrend("", plotpage.BadgeError),
		plotpage.NewStat("High Risk", strconv.Itoa(high)).WithTrend("", plotpage.BadgeWarning),
		plotpage.NewStat("Medium Risk", strconv.Itoa(medium)).WithTrend("", plotpage.BadgeInfo),
		plotpage.NewStat("Low Risk", strconv.Itoa(low)).WithTrend("", plotpage.BadgeSuccess),
	)

	return grid.Render(w)
}

func (bf *busfactorContent) countByRiskLevel() (critical, high, medium, low int) {
	for _, bfd := range bf.data.Metrics.BusFactor {
		switch bfd.RiskLevel {
		case RiskCritical:
			critical++
		case RiskHigh:
			high++
		case RiskMedium:
			medium++
		case RiskLow:
			low++
		}
	}

	return critical, high, medium, low
}

func (bf *busfactorContent) renderTable(w io.Writer) error {
	_, err := w.Write([]byte(`<div class="mt-6">`))
	if err != nil {
		return fmt.Errorf("writing table div: %w", err)
	}

	card := plotpage.NewCard("Bus Factor Analysis", "Risk assessment by language ownership concentration (CHAOSS methodology)")
	table := plotpage.NewTable([]string{
		"Language", "Risk Level", "Bus Factor", "Primary Owner", "Primary %", "Secondary Owner", "Secondary %",
	})

	count := min(riskTableMaxRows, len(bf.data.Metrics.BusFactor))
	for i := range count {
		bfd := bf.data.Metrics.BusFactor[i]
		table.AddRow(
			bfd.Language,
			riskBadgeHTML(bfd.RiskLevel),
			fmt.Sprintf("%d/%d", bfd.BusFactor, bfd.TotalContributors),
			bfd.PrimaryDevName,
			formatPercent(bfd.PrimaryPct),
			secondaryDevDisplay(bfd.SecondaryDevName),
			secondaryPctDisplay(bfd.SecondaryPct),
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

func riskBadgeHTML(level string) string {
	badge := plotpage.NewBadge(level)

	switch level {
	case RiskCritical:
		badge.WithColor(plotpage.BadgeError)
	case RiskHigh:
		badge.WithColor(plotpage.BadgeWarning)
	case RiskMedium:
		badge.WithColor(plotpage.BadgeInfo)
	default:
		badge.WithColor(plotpage.BadgeSuccess)
	}

	var buf bytes.Buffer

	err := badge.Render(&buf)
	if err != nil {
		return level
	}

	return buf.String()
}

func formatPercent(pct float64) string {
	return strconv.FormatFloat(pct, 'f', 1, 64) + "%"
}

func secondaryDevDisplay(name string) string {
	if name == "" {
		return "-"
	}

	return name
}

func secondaryPctDisplay(pct float64) string {
	if pct == 0 {
		return "-"
	}

	return formatPercent(pct)
}

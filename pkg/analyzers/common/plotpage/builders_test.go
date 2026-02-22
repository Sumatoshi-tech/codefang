package plotpage_test

import (
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

func TestBuildBarChart(t *testing.T) {
	t.Parallel()

	opts := plotpage.DefaultChartOpts()
	labels := []string{"Q1", "Q2", "Q3", "Q4"}
	series := []plotpage.BarSeries{
		{
			Name:  "Revenue",
			Data:  []plotpage.SeriesData{100, 200, 300, 400},
			Color: "#ff0000",
		},
		{
			Name: "Profit",
			Data: []plotpage.SeriesData{50, 100, 150, 200},
		},
	}

	chart := plotpage.BuildBarChart(opts, labels, series, "USD")
	require.NotNil(t, chart)
	require.NotEmpty(t, chart.MultiSeries)
	require.Len(t, chart.MultiSeries, 2)
	require.Equal(t, "Revenue", chart.MultiSeries[0].Name)
	require.Equal(t, "Profit", chart.MultiSeries[1].Name)
}

func TestBuildBarChart_NilOpts(t *testing.T) {
	t.Parallel()

	labels := []string{"Q1"}
	series := []plotpage.BarSeries{
		{Name: "Data", Data: []plotpage.SeriesData{100}},
	}

	chart := plotpage.BuildBarChart(nil, labels, series, "Count")
	require.NotNil(t, chart)
	require.Len(t, chart.MultiSeries, 1)
}

func TestBuildLineChart(t *testing.T) {
	t.Parallel()

	opts := plotpage.DefaultChartOpts()
	labels := []string{"Mon", "Tue", "Wed"}
	series := []plotpage.LineSeries{
		{
			Name:  "Active Users",
			Data:  []plotpage.SeriesData{10.5, 20.1, 15.0},
			Color: "#00ff00",
		},
	}

	chart := plotpage.BuildLineChart(opts, labels, series, "Users (k)")
	require.NotNil(t, chart)
	require.NotEmpty(t, chart.MultiSeries)
	require.Len(t, chart.MultiSeries, 1)
	require.Equal(t, "Active Users", chart.MultiSeries[0].Name)
}

func TestBuildLineChart_NilOpts(t *testing.T) {
	t.Parallel()

	labels := []string{"Jan"}
	series := []plotpage.LineSeries{
		{Name: "Data", Data: []plotpage.SeriesData{100}},
	}

	chart := plotpage.BuildLineChart(nil, labels, series, "Count")
	require.NotNil(t, chart)
	require.Len(t, chart.MultiSeries, 1)
}

package analyze

import (
	"testing"

	"github.com/stretchr/testify/require"
)

func TestNormalizeFormat_BinAlias(t *testing.T) {
	t.Parallel()

	require.Equal(t, FormatBinary, NormalizeFormat(" bin "))
}

func TestValidateUniversalFormat(t *testing.T) {
	t.Parallel()

	testCases := []struct {
		name   string
		format string
	}{
		{name: "json", format: FormatJSON},
		{name: "yaml", format: FormatYAML},
		{name: "plot", format: FormatPlot},
		{name: "binary", format: FormatBinary},
		{name: "bin alias", format: FormatBinAlias},
		{name: "timeseries", format: FormatTimeSeries},
		{name: "ndjson", format: FormatNDJSON},
		{name: "text", format: FormatText},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			t.Parallel()

			normalized, err := ValidateUniversalFormat(testCase.format)
			require.NoError(t, err)
			require.Equal(t, NormalizeFormat(testCase.format), normalized)
		})
	}
}

func TestValidateUniversalFormat_Invalid(t *testing.T) {
	t.Parallel()

	_, err := ValidateUniversalFormat("html")
	require.ErrorIs(t, err, ErrUnsupportedFormat)
}

func TestValidateFormat_CustomSet(t *testing.T) {
	t.Parallel()

	normalized, err := ValidateFormat(" BIN ", []string{FormatJSON, FormatBinary})
	require.NoError(t, err)
	require.Equal(t, FormatBinary, normalized)
}

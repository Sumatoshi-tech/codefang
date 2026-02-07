package streaming

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestParseMode_Auto(t *testing.T) {
	mode, err := ParseMode("auto")
	require.NoError(t, err)
	assert.Equal(t, ModeAuto, mode)
}

func TestParseMode_On(t *testing.T) {
	mode, err := ParseMode("on")
	require.NoError(t, err)
	assert.Equal(t, ModeOn, mode)
}

func TestParseMode_Off(t *testing.T) {
	mode, err := ParseMode("off")
	require.NoError(t, err)
	assert.Equal(t, ModeOff, mode)
}

func TestParseMode_Invalid(t *testing.T) {
	_, err := ParseMode("invalid")
	assert.ErrorIs(t, err, ErrInvalidMode)
}

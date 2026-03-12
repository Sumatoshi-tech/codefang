package analyze

import (
	"fmt"
	"strings"
	"unicode"
)

const normalizeExtraCapacity = 4

// NewDescriptor builds stable analyzer metadata from analyzer name and mode.
func NewDescriptor(mode AnalyzerMode, name, description string) Descriptor {
	return Descriptor{
		ID:          fmt.Sprintf("%s/%s", mode, normalizeName(name)),
		Description: description,
		Mode:        mode,
	}
}

func normalizeName(name string) string {
	normalized := strings.TrimSpace(name)
	if normalized == "" {
		return ""
	}

	builder := strings.Builder{}
	builder.Grow(len(normalized) + normalizeExtraCapacity)

	previousLower := false

	for _, current := range normalized {
		if current == '_' || current == ' ' {
			builder.WriteRune('-')

			previousLower = false

			continue
		}

		if unicode.IsUpper(current) {
			if previousLower {
				builder.WriteRune('-')
			}

			builder.WriteRune(unicode.ToLower(current))

			previousLower = false

			continue
		}

		builder.WriteRune(unicode.ToLower(current))
		previousLower = unicode.IsLetter(current) && unicode.IsLower(current)
	}

	return strings.Trim(builder.String(), "-")
}

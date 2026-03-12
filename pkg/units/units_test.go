package units

import "testing"

// FRD: specs/frds/FRD-20260302-size-unit-constants.md.

// Expected binary size multiplier values.
const (
	expectedKiB = 1024
	expectedMiB = 1024 * 1024
	expectedGiB = 1024 * 1024 * 1024
)

func TestBinarySizeConstants(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		got  int64
		want int64
	}{
		{"KiB equals 1024", KiB, expectedKiB},
		{"MiB equals 1024*KiB", MiB, expectedMiB},
		{"GiB equals 1024*MiB", GiB, expectedGiB},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			if tt.got != tt.want {
				t.Errorf("got %d, want %d", tt.got, tt.want)
			}
		})
	}
}

func TestBinarySizeRelationships(t *testing.T) {
	t.Parallel()

	t.Run("MiB is 1024 KiB", func(t *testing.T) {
		t.Parallel()

		const kiBPerMiB = 1024
		if MiB != kiBPerMiB*KiB {
			t.Errorf("MiB (%d) != 1024*KiB (%d)", MiB, kiBPerMiB*KiB)
		}
	})

	t.Run("GiB is 1024 MiB", func(t *testing.T) {
		t.Parallel()

		const miBPerGiB = 1024
		if GiB != miBPerGiB*MiB {
			t.Errorf("GiB (%d) != 1024*MiB (%d)", GiB, miBPerGiB*MiB)
		}
	})
}

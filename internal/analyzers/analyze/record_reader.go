package analyze

import "slices"

// ReadRecordsIfPresent reads all records of the given kind from reader,
// gob-decoding each into T and returning the collected slice.
// Returns (nil, nil) if kind is not present in kinds.
func ReadRecordsIfPresent[T any](reader ReportReader, kinds []string, kind string) ([]T, error) {
	if !slices.Contains(kinds, kind) {
		return nil, nil
	}

	var result []T

	iterErr := reader.Iter(kind, func(raw []byte) error {
		var record T

		decErr := GobDecode(raw, &record)
		if decErr != nil {
			return decErr
		}

		result = append(result, record)

		return nil
	})

	return result, iterErr
}

// ReadRecordIfPresent reads a single record of the given kind from reader.
// If multiple records exist, the last one wins.
// Returns (zero, nil) if kind is not present in kinds.
func ReadRecordIfPresent[T any](reader ReportReader, kinds []string, kind string) (T, error) {
	var result T

	if !slices.Contains(kinds, kind) {
		return result, nil
	}

	iterErr := reader.Iter(kind, func(raw []byte) error {
		return GobDecode(raw, &result)
	})

	return result, iterErr
}

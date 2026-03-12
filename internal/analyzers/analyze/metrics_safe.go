package analyze

// SafeMetricComputer wraps a [MetricComputer] to return the empty value when
// the report is empty, avoiding nil-pointer panics or meaningless computation
// in downstream metric logic. Non-empty reports are forwarded to compute.
func SafeMetricComputer[M any](compute MetricComputer[M], empty M) MetricComputer[M] {
	return func(report Report) (M, error) {
		if len(report) == 0 {
			return empty, nil
		}

		return compute(report)
	}
}

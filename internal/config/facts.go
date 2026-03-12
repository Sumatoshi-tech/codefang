package config

// positive constrains types eligible for skip-on-zero fact application.
type positive interface {
	~int | ~float32
}

// applyPositive sets facts[key] = value when value is positive.
// Zero values are skipped, allowing the analyzer to use its built-in default.
func applyPositive[T positive](facts map[string]any, key string, value T) {
	if value > 0 {
		facts[key] = value
	}
}

// applyNonEmpty sets facts[key] = value when value is non-empty.
func applyNonEmpty(facts map[string]any, key, value string) {
	if value != "" {
		facts[key] = value
	}
}

// applyBool sets facts[key] = value unconditionally.
// Boolean config fields are always applied because false is a meaningful override.
func applyBool(facts map[string]any, key string, value bool) {
	facts[key] = value
}

// ApplyToFacts merges config values into the analyzer facts map.
// Only non-zero config values override existing facts; zero values
// indicate "use analyzer default" and are skipped.
// Boolean fields are always applied because false is a meaningful value.
func (c *Config) ApplyToFacts(facts map[string]any) {
	bd := c.History.Burndown

	applyPositive(facts, "Burndown.Granularity", bd.Granularity)
	applyPositive(facts, "Burndown.Sampling", bd.Sampling)
	applyBool(facts, "Burndown.TrackFiles", bd.TrackFiles)
	applyBool(facts, "Burndown.TrackPeople", bd.TrackPeople)
	applyPositive(facts, "Burndown.HibernationThreshold", bd.HibernationThreshold)
	applyBool(facts, "Burndown.HibernationOnDisk", bd.HibernationToDisk)
	applyNonEmpty(facts, "Burndown.HibernationDirectory", bd.HibernationDirectory)
	applyBool(facts, "Burndown.Debug", bd.Debug)
	applyPositive(facts, "Burndown.Goroutines", bd.Goroutines)

	dv := c.History.Devs

	applyBool(facts, "Devs.ConsiderEmptyCommits", dv.ConsiderEmptyCommits)
	applyBool(facts, "Devs.Anonymize", dv.Anonymize)

	im := c.History.Imports

	applyPositive(facts, "Imports.Goroutines", im.Goroutines)
	applyPositive(facts, "Imports.MaxFileSize", im.MaxFileSize)

	se := c.History.Sentiment

	applyPositive(facts, "CommentSentiment.MinLength", se.MinCommentLength)
	applyPositive(facts, "CommentSentiment.Gap", float32(se.Gap))

	sh := c.History.Shotness

	applyNonEmpty(facts, "Shotness.DSLStruct", sh.DSLStruct)
	applyNonEmpty(facts, "Shotness.DSLName", sh.DSLName)

	applyPositive(facts, "TyposDatasetBuilder.MaximumAllowedDistance", c.History.Typos.MaxDistance)

	an := c.History.Anomaly

	applyPositive(facts, "TemporalAnomaly.Threshold", float32(an.Threshold))
	applyPositive(facts, "TemporalAnomaly.WindowSize", an.WindowSize)
}

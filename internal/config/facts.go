package config

// positive constrains types eligible for skip-on-zero fact application.
type positive interface {
	~int | ~float32 | ~float64
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
	c.applyBurndownFacts(facts)
	c.applyCouplesFacts(facts)
	c.applyDevsFacts(facts)
	c.applyFileHistoryFacts(facts)
	c.applyImportsFacts(facts)
	c.applySentimentFacts(facts)
	c.applyShotnessFacts(facts)
	c.applyTyposFacts(facts)
	c.applyAnomalyFacts(facts)
	c.applyClonesFacts(facts)
}

func (c *Config) applyBurndownFacts(facts map[string]any) {
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
}

func (c *Config) applyCouplesFacts(facts map[string]any) {
	cp := c.History.Couples

	applyPositive(facts, "Couples.CouplingThresholdHigh", cp.CouplingThresholdHigh)
	applyPositive(facts, "Couples.OwnershipFewThreshold", cp.OwnershipFewThreshold)
	applyPositive(facts, "Couples.OwnershipModerateThreshold", cp.OwnershipModerateThreshold)
	applyPositive(facts, "Couples.BatchCouplingThreshold", cp.BatchCouplingThreshold)
	applyPositive(facts, "Couples.HLLPrecision", cp.HLLPrecision)
	applyPositive(facts, "Couples.TopKPerFile", cp.TopKPerFile)
	applyPositive(facts, "Couples.MinEdgeWeight", cp.MinEdgeWeight)
}

func (c *Config) applyDevsFacts(facts map[string]any) {
	dv := c.History.Devs

	applyBool(facts, "Devs.ConsiderEmptyCommits", dv.ConsiderEmptyCommits)
	applyBool(facts, "Devs.Anonymize", dv.Anonymize)
	applyPositive(facts, "Devs.BusFactorThreshold", dv.BusFactorThreshold)
	applyPositive(facts, "Devs.RiskThresholdCritical", dv.RiskThresholdCritical)
	applyPositive(facts, "Devs.RiskThresholdHigh", dv.RiskThresholdHigh)
	applyPositive(facts, "Devs.RiskThresholdMedium", dv.RiskThresholdMedium)
	applyPositive(facts, "Devs.ActiveThresholdRatio", dv.ActiveThresholdRatio)
	applyPositive(facts, "Devs.DefaultActiveDays", dv.DefaultActiveDays)
	applyPositive(facts, "Devs.HLLPrecision", dv.HLLPrecision)
}

func (c *Config) applyFileHistoryFacts(facts map[string]any) {
	fh := c.History.FileHistory

	applyPositive(facts, "FileHistory.HotspotThresholdCritical", fh.HotspotThresholdCritical)
	applyPositive(facts, "FileHistory.HotspotThresholdHigh", fh.HotspotThresholdHigh)
	applyPositive(facts, "FileHistory.HotspotThresholdMedium", fh.HotspotThresholdMedium)
}

func (c *Config) applyImportsFacts(facts map[string]any) {
	im := c.History.Imports

	applyPositive(facts, "Imports.Goroutines", im.Goroutines)
	applyPositive(facts, "Imports.MaxFileSize", im.MaxFileSize)
	applyPositive(facts, "Imports.MaxDependencyRiskRows", im.MaxDependencyRiskRows)
}

func (c *Config) applySentimentFacts(facts map[string]any) {
	se := c.History.Sentiment

	applyPositive(facts, "CommentSentiment.MinLength", se.MinCommentLength)
	applyPositive(facts, "CommentSentiment.Gap", se.Gap)
	applyPositive(facts, "CommentSentiment.NeutralizerWeight", se.NeutralizerWeight)
	applyPositive(facts, "CommentSentiment.MaxWeightRatio", se.MaxWeightRatio)
	applyPositive(facts, "CommentSentiment.PositiveThreshold", se.PositiveThreshold)
	applyPositive(facts, "CommentSentiment.NegativeThreshold", se.NegativeThreshold)
	applyPositive(facts, "CommentSentiment.TrendThreshold", se.TrendThreshold)
	applyPositive(facts, "CommentSentiment.LowSentimentRiskThreshold", se.LowSentimentRiskThresh)
}

func (c *Config) applyShotnessFacts(facts map[string]any) {
	sh := c.History.Shotness

	applyNonEmpty(facts, "Shotness.DSLStruct", sh.DSLStruct)
	applyNonEmpty(facts, "Shotness.DSLName", sh.DSLName)
}

func (c *Config) applyTyposFacts(facts map[string]any) {
	applyPositive(facts, "TyposDatasetBuilder.MaximumAllowedDistance", c.History.Typos.MaxDistance)
}

func (c *Config) applyAnomalyFacts(facts map[string]any) {
	an := c.History.Anomaly

	applyPositive(facts, "TemporalAnomaly.Threshold", an.Threshold)
	applyPositive(facts, "TemporalAnomaly.WindowSize", an.WindowSize)
}

func (c *Config) applyClonesFacts(facts map[string]any) {
	cl := c.History.Clones

	applyPositive(facts, "Clones.MaxClonePairs", cl.MaxClonePairs)
	applyPositive(facts, "Clones.NumHashes", cl.NumHashes)
	applyPositive(facts, "Clones.NumBands", cl.NumBands)
	applyPositive(facts, "Clones.NumRows", cl.NumRows)
	applyPositive(facts, "Clones.ShingleSize", cl.ShingleSize)
	applyPositive(facts, "Clones.SimilarityType2", cl.SimilarityType2)
	applyPositive(facts, "Clones.SimilarityType3", cl.SimilarityType3)
	applyPositive(facts, "Clones.ThresholdRatioYellow", cl.ThresholdRatioYellow)
	applyPositive(facts, "Clones.ThresholdRatioRed", cl.ThresholdRatioRed)
	applyPositive(facts, "Clones.ThresholdPairsYellow", cl.ThresholdPairsYellow)
	applyPositive(facts, "Clones.ThresholdPairsRed", cl.ThresholdPairsRed)
}

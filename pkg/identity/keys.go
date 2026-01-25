package identity

const (
	// AuthorMissing is the internal author index which denotes any unmatched identities
	// (Detector.Consume()). It may *not* be (1 << 18) - 1, see BurndownAnalysis.packPersonWithDay().
	AuthorMissing = (1 << 18) - 2
	// AuthorMissingName is the string name which corresponds to AuthorMissing.
	AuthorMissingName = "<unmatched>"

	// FactIdentityDetectorPeopleDict is the name of the fact which is inserted in
	// Detector.Configure(). It corresponds to Detector.PeopleDict - the mapping
	// from the signatures to the author indices.
	FactIdentityDetectorPeopleDict = "IdentityDetector.PeopleDict"
	// FactIdentityDetectorReversedPeopleDict is the name of the fact which is inserted in
	// Detector.Configure(). It corresponds to Detector.ReversedPeopleDict -
	// the mapping from the author indices to the main signature.
	FactIdentityDetectorReversedPeopleDict = "IdentityDetector.ReversedPeopleDict"
	// ConfigIdentityDetectorPeopleDictPath is the name of the configuration option
	// (Detector.Configure()) which allows to set the external PeopleDict mapping from a file.
	ConfigIdentityDetectorPeopleDictPath = "IdentityDetector.PeopleDictPath"
	// ConfigIdentityDetectorExactSignatures is the name of the configuration option
	// (Detector.Configure()) which changes the matching algorithm to exact signature (name + email)
	// correspondence.
	ConfigIdentityDetectorExactSignatures = "IdentityDetector.ExactSignatures"
	// FactIdentityDetectorPeopleCount is the name of the fact which is inserted in
	// Detector.Configure(). It is equal to the overall number of unique authors
	// (the length of ReversedPeopleDict).
	FactIdentityDetectorPeopleCount = "IdentityDetector.PeopleCount"

	// DependencyAuthor is the name of the dependency provided by Detector.
	DependencyAuthor = "author"
)

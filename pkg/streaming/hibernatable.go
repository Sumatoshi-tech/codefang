package streaming

// Hibernatable is an optional interface for analyzers that support hibernation.
// Analyzers implementing this interface can have their state compressed between
// chunks to reduce memory usage during streaming execution.
type Hibernatable interface {
	// Hibernate compresses the analyzer's state to reduce memory usage.
	// Called between chunks during streaming execution.
	Hibernate() error

	// Boot restores the analyzer from hibernated state.
	// Called before processing a new chunk after hibernation.
	Boot() error
}

// hibernateAll calls Hibernate on all hibernatable analyzers.
func hibernateAll(analyzers []Hibernatable) error {
	for _, h := range analyzers {
		err := h.Hibernate()
		if err != nil {
			return err
		}
	}

	return nil
}

// bootAll calls Boot on all hibernatable analyzers.
func bootAll(analyzers []Hibernatable) error {
	for _, h := range analyzers {
		err := h.Boot()
		if err != nil {
			return err
		}
	}

	return nil
}

package gitlib

import "time"

// Signature represents a git signature (author/committer).
type Signature struct {
	Name  string
	Email string
	When  time.Time
}

package identity_test

import (
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/identity"
)

const (
	testName  = "daniel smith"
	testEmail = "dbsmith@google.com"
)

func TestSplitIdentity_PipeDelimited(t *testing.T) {
	t.Parallel()

	name, email := identity.SplitIdentity("daniel smith|dbsmith@google.com")

	assert.Equal(t, testName, name)
	assert.Equal(t, testEmail, email)
}

func TestSplitIdentity_ExactFormat(t *testing.T) {
	t.Parallel()

	name, email := identity.SplitIdentity("daniel smith <dbsmith@google.com>")

	assert.Equal(t, testName, name)
	assert.Equal(t, testEmail, email)
}

func TestSplitIdentity_NameOnly(t *testing.T) {
	t.Parallel()

	name, email := identity.SplitIdentity("daniel smith")

	assert.Equal(t, testName, name)
	assert.Empty(t, email)
}

func TestSplitIdentity_Empty(t *testing.T) {
	t.Parallel()

	name, email := identity.SplitIdentity("")

	assert.Empty(t, name)
	assert.Empty(t, email)
}

func TestSplitIdentity_MultipleAliases(t *testing.T) {
	t.Parallel()

	name, email := identity.SplitIdentity("alice|bob|alice@example.com|bob@example.com")

	assert.Equal(t, "alice", name)
	assert.Equal(t, "alice@example.com", email)
}

func TestSplitIdentity_UnmatchedAuthor(t *testing.T) {
	t.Parallel()

	name, email := identity.SplitIdentity(identity.AuthorMissingName)

	assert.Equal(t, identity.AuthorMissingName, name)
	assert.Empty(t, email)
}

package plumbing

import (
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// ChangeRouter routes tree-diff changes to appropriate handlers.
type ChangeRouter struct {
	OnInsert func(change *gitlib.Change) error
	OnDelete func(change *gitlib.Change) error
	OnModify func(change *gitlib.Change) error
	OnRename func(from, to string, change *gitlib.Change) error
}

// Route iterates over the given changes and delegates to the configured handlers.
func (r *ChangeRouter) Route(changes gitlib.Changes) error {
	for _, change := range changes {
		err := r.routeChange(change)
		if err != nil {
			return err
		}
	}

	return nil
}

// routeChange dispatches a single change to the appropriate handler.
func (r *ChangeRouter) routeChange(change *gitlib.Change) error {
	if change.Action == gitlib.Modify && change.From.Name != change.To.Name {
		return r.handleRename(change)
	}

	switch change.Action {
	case gitlib.Insert:
		return callChangeHandler(r.OnInsert, change)
	case gitlib.Delete:
		return callChangeHandler(r.OnDelete, change)
	case gitlib.Modify:
		return callChangeHandler(r.OnModify, change)
	}

	return nil
}

// handleRename invokes the OnRename handler if it is non-nil.
func (r *ChangeRouter) handleRename(change *gitlib.Change) error {
	if r.OnRename == nil {
		return nil
	}

	return r.OnRename(change.From.Name, change.To.Name, change)
}

// callChangeHandler invokes a handler function if it is non-nil.
func callChangeHandler(handler func(*gitlib.Change) error, change *gitlib.Change) error {
	if handler == nil {
		return nil
	}

	return handler(change)
}

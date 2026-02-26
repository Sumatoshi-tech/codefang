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
//
//nolint:gocognit // Routing logic has high cognitive complexity.
func (r *ChangeRouter) Route(changes gitlib.Changes) error {
	for _, change := range changes {
		if change.Action == gitlib.Modify && change.From.Name != change.To.Name {
			if r.OnRename != nil {
				err := r.OnRename(change.From.Name, change.To.Name, change)
				if err != nil {
					return err
				}
			}

			continue
		}

		switch change.Action {
		case gitlib.Insert:
			if r.OnInsert != nil {
				err := r.OnInsert(change)
				if err != nil {
					return err
				}
			}
		case gitlib.Delete:
			if r.OnDelete != nil {
				err := r.OnDelete(change)
				if err != nil {
					return err
				}
			}
		case gitlib.Modify:
			if r.OnModify != nil {
				err := r.OnModify(change)
				if err != nil {
					return err
				}
			}
		}
	}

	return nil
}

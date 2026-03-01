package analyze

import (
	"bufio"
	"encoding/gob"
	"errors"
	"fmt"
	"io"
	"iter"
	"os"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// SpilledUASTRecord is the gob-serialized record for one UAST file change.
// ChangeIndex references into the CommitData.Changes slice to reconstruct
// the *gitlib.Change pointer on deserialization.
type SpilledUASTRecord struct {
	ChangeIndex int
	Before      *node.Node
	After       *node.Node
}

// EncodeUASTRecord writes a single UAST change record to the gob encoder.
func EncodeUASTRecord(enc *gob.Encoder, changeIndex int, before, after *node.Node) error {
	err := enc.Encode(SpilledUASTRecord{
		ChangeIndex: changeIndex,
		Before:      before,
		After:       after,
	})
	if err != nil {
		return fmt.Errorf("uast spill encode: %w", err)
	}

	return nil
}

// StreamUASTChanges deserializes a spill file and yields changes one by one.
// Each record's ChangeIndex is used to reconstruct the *gitlib.Change pointer
// from the provided changes slice.
func StreamUASTChanges(path string, changes gitlib.Changes) iter.Seq[uast.Change] {
	return func(yield func(uast.Change) bool) {
		f, err := os.Open(path)
		if err != nil {
			return
		}
		defer f.Close()

		dec := gob.NewDecoder(bufio.NewReader(f))
		decodeLoop(dec, changes, yield)
	}
}

func decodeLoop(dec *gob.Decoder, changes gitlib.Changes, yield func(uast.Change) bool) {
	for {
		var rec SpilledUASTRecord

		err := dec.Decode(&rec)
		if errors.Is(err, io.EOF) {
			return
		}

		if err != nil {
			return
		}

		if rec.ChangeIndex < 0 || rec.ChangeIndex >= len(changes) {
			node.ReleaseTree(rec.Before)
			node.ReleaseTree(rec.After)

			continue
		}

		change := uast.Change{
			Before: rec.Before,
			After:  rec.After,
			Change: changes[rec.ChangeIndex],
		}

		if !yield(change) {
			node.ReleaseTree(rec.Before)
			node.ReleaseTree(rec.After)

			return
		}

		node.ReleaseTree(rec.Before)
		node.ReleaseTree(rec.After)
	}
}

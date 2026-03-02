package analyze

import (
	"encoding/json"
	"fmt"
	"io"
	"sync"
	"time"
)

// TCSink is a callback that receives stamped TCs during pipeline execution.
// Used by the NDJSON streaming output to write one JSON line per TC.
type TCSink func(tc TC, analyzerFlag string) error

// NDJSONLine is the JSON structure for one NDJSON output line.
type NDJSONLine struct {
	Hash      string `json:"hash"`
	Tick      int    `json:"tick"`
	AuthorID  int    `json:"author_id"`
	Timestamp string `json:"timestamp"`
	Analyzer  string `json:"analyzer"`
	Data      any    `json:"data"`
}

// StreamingSink writes one NDJSON line per TC to an [io.Writer].
// Thread-safe: concurrent WriteTC calls are serialized via a mutex.
type StreamingSink struct {
	mu      sync.Mutex
	encoder *json.Encoder
}

// NewStreamingSink creates a StreamingSink that writes to the given writer.
func NewStreamingSink(w io.Writer) *StreamingSink {
	return &StreamingSink{
		encoder: json.NewEncoder(w),
	}
}

// WriteTC writes one NDJSON line for the given TC. Skips TCs with nil Data.
func (s *StreamingSink) WriteTC(tc TC, analyzerFlag string) error {
	if tc.Data == nil {
		return nil
	}

	var ts string
	if !tc.Timestamp.IsZero() {
		ts = tc.Timestamp.Format(time.RFC3339)
	}

	line := NDJSONLine{
		Hash:      tc.CommitHash.String(),
		Tick:      tc.Tick,
		AuthorID:  tc.AuthorID,
		Timestamp: ts,
		Analyzer:  analyzerFlag,
		Data:      tc.Data,
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	err := s.encoder.Encode(line)
	if err != nil {
		return fmt.Errorf("ndjson encode: %w", err)
	}

	return nil
}

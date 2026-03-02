package analyze

import (
	"bytes"
	"encoding/gob"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"sort"
	"strings"
	"sync"
)

const (
	manifestFile = "manifest.json"
	metaFile     = "meta.json"
	gobExtension = ".gob"
	tmpExtension = ".tmp"
	dirPerm      = 0o750
	filePerm     = 0o600
)

// ErrAnalyzerNotFound is returned when opening a non-existent analyzer.
var ErrAnalyzerNotFound = errors.New("analyzer not found in store")

// ErrTornWrite is returned when a write was not finalized.
var ErrTornWrite = errors.New("torn write detected: writer was not closed")

// ErrWriterClosed is returned when writing to a closed writer.
var ErrWriterClosed = errors.New("report writer: write after close")

// storeManifest persists the ordered list of analyzer IDs.
type storeManifest struct {
	AnalyzerIDs []string `json:"analyzer_ids"`
}

// FileReportStore is a file-backed [ReportStore] using gob encoding.
// Directory layout: manifest.json + per-analyzer subdirectories with
// meta.json and <kind>.gob files.
type FileReportStore struct {
	dir      string
	mu       sync.Mutex
	manifest storeManifest
}

// NewFileReportStore creates a file-backed [ReportStore] rooted at dir.
// If the directory already contains a manifest, it is loaded so that
// AnalyzerIDs returns the stored list without requiring new writes.
func NewFileReportStore(dir string) *FileReportStore {
	s := &FileReportStore{dir: dir}
	s.loadManifest()

	return s
}

// loadManifest reads the manifest from disk if it exists.
// Errors are silently ignored — a missing manifest simply means the store
// is fresh (write scenario).
func (s *FileReportStore) loadManifest() {
	manifestPath := filepath.Join(s.dir, manifestFile)

	data, readErr := os.ReadFile(manifestPath)
	if readErr != nil {
		return
	}

	unmarshalErr := json.Unmarshal(data, &s.manifest)
	if unmarshalErr != nil {
		s.manifest = storeManifest{}
	}
}

// Begin starts writing records for the given analyzer.
func (s *FileReportStore) Begin(analyzerID string, meta ReportMeta) (ReportWriter, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	analyzerDir := s.analyzerDir(analyzerID)

	mkErr := os.MkdirAll(analyzerDir, dirPerm)
	if mkErr != nil {
		return nil, fmt.Errorf("report store begin: %w", mkErr)
	}

	metaBytes, marshalErr := json.Marshal(meta)
	if marshalErr != nil {
		return nil, fmt.Errorf("report store marshal meta: %w", marshalErr)
	}

	metaPath := filepath.Join(analyzerDir, metaFile)

	writeErr := os.WriteFile(metaPath, metaBytes, filePerm)
	if writeErr != nil {
		return nil, fmt.Errorf("report store write meta: %w", writeErr)
	}

	return &fileReportWriter{
		dir:        analyzerDir,
		analyzerID: analyzerID,
		store:      s,
		writers:    make(map[string]*gobFileWriter),
	}, nil
}

// Open returns a reader for the given analyzer's stored records.
func (s *FileReportStore) Open(analyzerID string) (ReportReader, error) {
	analyzerDir := s.analyzerDir(analyzerID)
	metaPath := filepath.Join(analyzerDir, metaFile)

	metaBytes, readErr := os.ReadFile(metaPath)
	if readErr != nil {
		if os.IsNotExist(readErr) {
			return nil, fmt.Errorf("%w: %s", ErrAnalyzerNotFound, analyzerID)
		}

		return nil, fmt.Errorf("report store open meta: %w", readErr)
	}

	entries, dirErr := os.ReadDir(analyzerDir)
	if dirErr != nil {
		return nil, fmt.Errorf("report store read dir: %w", dirErr)
	}

	// Torn write detection: any .tmp file means the writer was not closed.
	for _, entry := range entries {
		if strings.HasSuffix(entry.Name(), tmpExtension) {
			return nil, fmt.Errorf("%w: %s has uncommitted file %s", ErrTornWrite, analyzerID, entry.Name())
		}
	}

	var meta ReportMeta

	unmarshalErr := json.Unmarshal(metaBytes, &meta)
	if unmarshalErr != nil {
		return nil, fmt.Errorf("report store unmarshal meta: %w", unmarshalErr)
	}

	kinds := make([]string, 0)

	for _, entry := range entries {
		kind, ok := strings.CutSuffix(entry.Name(), gobExtension)
		if ok {
			kinds = append(kinds, kind)
		}
	}

	sort.Strings(kinds)

	return &fileReportReader{
		dir:   analyzerDir,
		meta:  meta,
		kinds: kinds,
	}, nil
}

// AnalyzerIDs returns the ordered list of analyzer IDs that have been written.
func (s *FileReportStore) AnalyzerIDs() []string {
	s.mu.Lock()
	defer s.mu.Unlock()

	return slices.Clone(s.manifest.AnalyzerIDs)
}

// Close releases store-level resources.
func (s *FileReportStore) Close() error {
	return nil
}

func (s *FileReportStore) analyzerDir(analyzerID string) string {
	safe := strings.ReplaceAll(analyzerID, "/", "_")

	return filepath.Join(s.dir, safe)
}

func (s *FileReportStore) addAnalyzerID(analyzerID string) error {
	if !slices.Contains(s.manifest.AnalyzerIDs, analyzerID) {
		s.manifest.AnalyzerIDs = append(s.manifest.AnalyzerIDs, analyzerID)
	}

	return s.writeManifest()
}

func (s *FileReportStore) writeManifest() error {
	data, marshalErr := json.Marshal(s.manifest)
	if marshalErr != nil {
		return fmt.Errorf("report store marshal manifest: %w", marshalErr)
	}

	manifestPath := filepath.Join(s.dir, manifestFile)
	tmpPath := manifestPath + tmpExtension

	writeErr := os.WriteFile(tmpPath, data, filePerm)
	if writeErr != nil {
		return fmt.Errorf("report store write manifest: %w", writeErr)
	}

	renameErr := os.Rename(tmpPath, manifestPath)
	if renameErr != nil {
		return fmt.Errorf("report store rename manifest: %w", renameErr)
	}

	return nil
}

// gobFileWriter buffers gob-encoded records for one kind.
type gobFileWriter struct {
	buf bytes.Buffer
	enc *gob.Encoder
}

// fileReportWriter implements [ReportWriter] for file-backed stores.
type fileReportWriter struct {
	dir        string
	analyzerID string
	store      *FileReportStore
	writers    map[string]*gobFileWriter
	closed     bool
}

// Write appends one typed record under the given kind.
func (w *fileReportWriter) Write(kind string, record any) error {
	if w.closed {
		return ErrWriterClosed
	}

	gw, ok := w.writers[kind]
	if !ok {
		gw = &gobFileWriter{}
		gw.enc = gob.NewEncoder(&gw.buf)
		w.writers[kind] = gw
	}

	// Pre-encode the record to bytes so the reader can stream raw bytes.
	var recBuf bytes.Buffer

	recErr := gob.NewEncoder(&recBuf).Encode(record)
	if recErr != nil {
		return fmt.Errorf("report writer encode record: %w", recErr)
	}

	frameErr := gw.enc.Encode(recBuf.Bytes())
	if frameErr != nil {
		return fmt.Errorf("report writer encode frame: %w", frameErr)
	}

	return nil
}

// Close finalizes the write atomically. Idempotent.
func (w *fileReportWriter) Close() error {
	if w.closed {
		return nil
	}

	w.closed = true

	for kind, gw := range w.writers {
		flushErr := w.flushKind(kind, gw)
		if flushErr != nil {
			return flushErr
		}
	}

	w.store.mu.Lock()
	defer w.store.mu.Unlock()

	return w.store.addAnalyzerID(w.analyzerID)
}

func (w *fileReportWriter) flushKind(kind string, gw *gobFileWriter) error {
	tmpPath := filepath.Join(w.dir, kind+tmpExtension)
	finalPath := filepath.Join(w.dir, kind+gobExtension)

	fd, createErr := os.Create(tmpPath)
	if createErr != nil {
		return fmt.Errorf("report writer create: %w", createErr)
	}

	_, copyErr := io.Copy(fd, &gw.buf)
	if copyErr != nil {
		fd.Close()

		return fmt.Errorf("report writer copy: %w", copyErr)
	}

	syncErr := fd.Sync()
	if syncErr != nil {
		fd.Close()

		return fmt.Errorf("report writer sync: %w", syncErr)
	}

	closeErr := fd.Close()
	if closeErr != nil {
		return fmt.Errorf("report writer close fd: %w", closeErr)
	}

	renameErr := os.Rename(tmpPath, finalPath)
	if renameErr != nil {
		return fmt.Errorf("report writer rename: %w", renameErr)
	}

	return nil
}

// fileReportReader implements [ReportReader] for file-backed stores.
type fileReportReader struct {
	dir   string
	meta  ReportMeta
	kinds []string
}

// Meta returns the metadata for this analyzer's report.
func (r *fileReportReader) Meta() ReportMeta {
	return r.meta
}

// Kinds returns the list of record kinds stored for this analyzer.
func (r *fileReportReader) Kinds() []string {
	return slices.Clone(r.kinds)
}

// Iter calls fn for each raw record of the given kind.
func (r *fileReportReader) Iter(kind string, fn func(raw []byte) error) error {
	gobPath := filepath.Join(r.dir, kind+gobExtension)

	fd, openErr := os.Open(gobPath)
	if openErr != nil {
		return fmt.Errorf("report reader open %s: %w", kind, openErr)
	}

	defer fd.Close()

	dec := gob.NewDecoder(fd)

	for {
		var raw []byte

		decErr := dec.Decode(&raw)
		if errors.Is(decErr, io.EOF) {
			return nil
		}

		if decErr != nil {
			return fmt.Errorf("report reader decode %s: %w", kind, decErr)
		}

		fnErr := fn(raw)
		if fnErr != nil {
			return fnErr
		}
	}
}

// Close releases reader resources. Idempotent.
func (r *fileReportReader) Close() error {
	return nil
}

// GobDecode decodes a gob-encoded byte slice into dst.
func GobDecode(raw []byte, dst any) error {
	decErr := gob.NewDecoder(bytes.NewReader(raw)).Decode(dst)
	if decErr != nil {
		return fmt.Errorf("gob decode: %w", decErr)
	}

	return nil
}

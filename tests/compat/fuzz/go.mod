// Standalone module for the Go-native differential fuzz targets. The main
// repository no longer carries a root go.mod (the Go implementation was
// superseded by the Rust rewrite), but this harness is deliberately written
// in Go: `testing/F` drives the LIVE frozen Go oracle binary and the Rust
// binary as subprocesses and diffs their output. Stdlib-only.
module compatfuzz

go 1.22

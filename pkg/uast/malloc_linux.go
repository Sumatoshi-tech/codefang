package uast

// #include <malloc.h>
import "C"

// MallocTrim returns freed C heap memory to the operating system.
// It addresses glibc ptmalloc per-thread arena fragmentation that occurs
// when goroutines call into CGO for tree-sitter parsing.
func MallocTrim() {
	C.malloc_trim(0)
}

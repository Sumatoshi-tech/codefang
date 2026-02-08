package uast

import "unsafe"

// tsNodeContext maps the first 4 uint32 fields of a tree-sitter TSNode.
// TSNode layout (from tree-sitter api.h):
//
//	struct TSNode {
//	  uint32_t context[4];  // [0]=start_byte, [1]=start_row, [2]=start_col, [3]=alias
//	  const void *id;
//	  const TSTree *tree;
//	};
//
// sitter.Node wraps this as: struct { c C.TSNode }.
// Since context[0..2] are plain integer fields stored directly in the struct,
// we can read them via unsafe without any CGO call.
type tsNodeContext struct {
	startByte uint32
	startRow  uint32
	startCol  uint32
	alias     uint32
}

// readStartPositions reads start byte/row/col directly from the TSNode struct
// via unsafe, bypassing CGO entirely. The nodePtr must point to a sitter.Node.
func readStartPositions(nodePtr unsafe.Pointer) (startByte, startRow, startCol uint) {
	ctx := (*tsNodeContext)(nodePtr)

	return uint(ctx.startByte), uint(ctx.startRow), uint(ctx.startCol)
}

// tsNodeFull maps the TSNode struct including the id pointer.
// TSNode layout (64-bit):
//
//	Offset  0: context[4] (16 bytes)
//	Offset 16: id (8 bytes, pointer to Subtree union)
//	Offset 24: tree (8 bytes, pointer to TSTree)
type tsNodeFull struct {
	context [4]uint32
	id      unsafe.Pointer
	tree    unsafe.Pointer
}

// subtreeHeapPartial mirrors the SubtreeHeapData C struct fields up to
// named_child_count. Verified via offsetof() on linux/amd64 gcc:
//
//	Offset  0: ref_count (uint32)
//	Offset  4: padding (Length = {uint32 bytes, TSPoint{uint32 row, uint32 col}} = 12 bytes)
//	Offset 16: size (Length = 12 bytes)
//	Offset 28: lookahead_bytes (uint32)
//	Offset 32: error_cost (uint32)
//	Offset 36: child_count (uint32)
//	Offset 40: symbol (uint16)
//	Offset 42: parse_state (uint16)
//	Offset 44: flags (bitfield, 2 bytes used + 2 padding = 4 bytes)
//	Offset 48: visible_child_count (uint32) — union field, valid when child_count > 0
//	Offset 52: named_child_count (uint32) — union field, valid when child_count > 0
type subtreeHeapPartial struct {
	refCount        uint32
	paddingBytes    uint32
	paddingRow      uint32
	paddingCol      uint32
	sizeBytes       uint32
	sizeRow         uint32
	sizeCol         uint32
	lookaheadBytes  uint32
	errorCost       uint32
	childCount      uint32
	symbol          uint16
	parseState      uint16
	flags           uint32
	visibleChildren uint32
	namedChildren   uint32
}

// readNamedChildCount reads the named child count directly from the TSNode's
// Subtree struct via unsafe, bypassing CGO entirely.
// Returns 0 for inline subtrees (leaf nodes) and for nodes with child_count == 0.
//
// TSNode.id is a pointer to a Subtree union (8 bytes):
//   - If is_inline (LSB of first byte = 1): SubtreeInlineData, no children.
//   - Otherwise: the 8 bytes ARE a const SubtreeHeapData* pointer.
func readNamedChildCount(nodePtr unsafe.Pointer) uint32 {
	full := (*tsNodeFull)(nodePtr)

	// TSNode.id points to a Subtree union. Check is_inline (LSB of first byte).
	firstByte := (*byte)(full.id)
	if *firstByte&1 == 1 {
		return 0
	}

	// Not inline: the Subtree union's 8 bytes are a pointer to SubtreeHeapData.
	heapPtrPtr := (*unsafe.Pointer)(full.id)
	heap := (*subtreeHeapPartial)(*heapPtrPtr)

	if heap.childCount == 0 {
		return 0
	}

	return heap.namedChildren
}

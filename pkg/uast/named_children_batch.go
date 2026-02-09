package uast

import (
	"unsafe"

	sitter "github.com/alexaandru/go-tree-sitter-bare"
)

func readNamedChildrenBatch(nodePtr unsafe.Pointer, children []batchChildInfo) (written, total uint32) {
	full := (*tsNodeFull)(nodePtr)
	if full.id == nil {
		return 0, 0
	}

	fillNamedChildrenBatchFromParts(
		full.context[0],
		full.context[1],
		full.context[2],
		full.context[3],
		uintptr(full.id),
		uintptr(full.tree),
		children,
		&written,
		&total,
	)

	return written, total
}

func batchChildToNode(child batchChildInfo) sitter.Node {
	raw := tsNodeFull{
		context: [4]uint32{uint32(child.ctx0), uint32(child.ctx1), uint32(child.ctx2), uint32(child.ctx3)},
		id:      child.id,
		tree:    child.tree,
	}

	return *(*sitter.Node)(unsafe.Pointer(&raw))
}

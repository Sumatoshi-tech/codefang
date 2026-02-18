package uast

/*
#include "cgo_named_children_batch.h"
*/
import "C"

type batchChildInfo C.cf_child_info

func fillNamedChildrenBatchFromParts(
	ctx0,
	ctx1,
	ctx2,
	ctx3 uint32,
	idRaw,
	treeRaw uintptr,
	children []batchChildInfo,
	written *uint32,
	total *uint32,
) {
	var totalNamed C.uint32_t
	var writtenNamed C.uint32_t
	var output *C.cf_child_info
	var outputCap C.uint32_t
	cCtx0 := C.uint32_t(ctx0)
	cCtx1 := C.uint32_t(ctx1)
	cCtx2 := C.uint32_t(ctx2)
	cCtx3 := C.uint32_t(ctx3)
	cIDRaw := C.uintptr_t(idRaw)
	cTreeRaw := C.uintptr_t(treeRaw)

	if len(children) > 0 {
		output = (*C.cf_child_info)(&children[0])
		outputCap = C.uint32_t(len(children))
	}

	C.cf_node_named_children_batch(cCtx0, cCtx1, cCtx2, cCtx3, cIDRaw, cTreeRaw, output, outputCap, &writtenNamed, &totalNamed)

	*written = uint32(writtenNamed)
	*total = uint32(totalNamed)
}

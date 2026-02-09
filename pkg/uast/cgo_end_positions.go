package uast

/*
#include <stdint.h>

typedef struct {
	uint32_t row;
	uint32_t column;
} TSPoint;

typedef struct {
	uint32_t context[4];
	const void *id;
	const void *tree;
} TSNode;

extern TSPoint ts_node_end_point(TSNode self);
extern uint32_t ts_node_end_byte(TSNode self);

static inline void cf_node_end_positions(
	uint32_t ctx0,
	uint32_t ctx1,
	uint32_t ctx2,
	uint32_t ctx3,
	uintptr_t id_raw,
	uintptr_t tree_raw,
	uint32_t *end_row,
	uint32_t *end_col,
	uint32_t *end_byte
) {
	TSNode node;
	TSPoint point;

	node.context[0] = ctx0;
	node.context[1] = ctx1;
	node.context[2] = ctx2;
	node.context[3] = ctx3;
	node.id = (const void *)id_raw;
	node.tree = (const void *)tree_raw;

	point = ts_node_end_point(node);
	*end_row = point.row;
	*end_col = point.column;
	*end_byte = ts_node_end_byte(node);
}
*/
import "C"

func readEndPositionsFromParts(
	ctx0, ctx1, ctx2, ctx3 uint32,
	idRaw, treeRaw uintptr,
) (endByte, endRow, endCol uint) {
	var (
		row       C.uint32_t
		col       C.uint32_t
		byteValue C.uint32_t
	)

	C.cf_node_end_positions(
		C.uint32_t(ctx0),
		C.uint32_t(ctx1),
		C.uint32_t(ctx2),
		C.uint32_t(ctx3),
		C.uintptr_t(idRaw),
		C.uintptr_t(treeRaw),
		&row,
		&col,
		&byteValue,
	)

	return uint(byteValue), uint(row), uint(col)
}

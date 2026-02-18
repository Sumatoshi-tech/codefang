#include "cgo_named_children_batch.h"
#include <stddef.h>

typedef struct {
	uint32_t context[4];
	const void *id;
	const void *tree;
} TSNode;

extern uint32_t ts_node_named_child_count(TSNode self);
extern TSNode ts_node_named_child(TSNode self, uint32_t child_index);
extern const char *ts_node_type(TSNode self);

void cf_node_named_children_batch(
	uint32_t ctx0,
	uint32_t ctx1,
	uint32_t ctx2,
	uint32_t ctx3,
	uintptr_t id_raw,
	uintptr_t tree_raw,
	cf_child_info *out,
	uint32_t cap,
	uint32_t *written,
	uint32_t *total_named
) {
	TSNode node;
	uint32_t named_count;
	uint32_t written_count = 0;

	node.context[0] = ctx0;
	node.context[1] = ctx1;
	node.context[2] = ctx2;
	node.context[3] = ctx3;
	node.id = (const void *)id_raw;
	node.tree = (const void *)tree_raw;

	named_count = ts_node_named_child_count(node);
	if (total_named != NULL) {
		*total_named = named_count;
	}

	if (written != NULL) {
		*written = 0;
	}

	if (out == NULL || cap == 0) {
		return;
	}

	if (named_count < cap) {
		cap = named_count;
	}

	written_count = cap;

	for (uint32_t idx = 0; idx < cap; idx++) {
		TSNode child = ts_node_named_child(node, idx);
		out[idx].ctx0 = child.context[0];
		out[idx].ctx1 = child.context[1];
		out[idx].ctx2 = child.context[2];
		out[idx].ctx3 = child.context[3];
		out[idx].id = child.id;
		out[idx].tree = child.tree;
		out[idx].type_ptr = ts_node_type(child);
		out[idx].named_child_count = ts_node_named_child_count(child);
	}

	if (written != NULL) {
		*written = written_count;
	}
}

#ifndef CODEFANG_UAST_CGO_NAMED_CHILDREN_BATCH_H
#define CODEFANG_UAST_CGO_NAMED_CHILDREN_BATCH_H

#include <stdint.h>

typedef struct {
	uint32_t ctx0;
	uint32_t ctx1;
	uint32_t ctx2;
	uint32_t ctx3;
	const void *id;
	const void *tree;
	const char *type_ptr;
	uint32_t named_child_count;
} cf_child_info;

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
);

#endif

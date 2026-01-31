/*
 * Codefang Git Library - Diff Operations
 *
 * Optimized for batch diff computation with:
 * 1. ODB-based blob preloading for better cache efficiency
 * 2. Sorted OID processing for pack locality
 * 3. Single ODB refresh per batch
 */

#include "codefang_git.h"
#include <stdlib.h>
#include <string.h>

/* Context for diff callbacks */
typedef struct {
    cf_diff_result* result;
    int current_type;
    int current_count;
    int old_line_pos;
    int new_line_pos;
} diff_ctx_t;

/* Flush pending diff operation to result */
static void flush_op(diff_ctx_t* ctx) {
    if (ctx->current_count > 0) {
        cf_diff_result* res = ctx->result;
        if (res->op_count < res->op_capacity) {
            res->ops[res->op_count].type_ = ctx->current_type;
            res->ops[res->op_count].line_count = ctx->current_count;
            res->op_count++;
        }
        ctx->current_count = 0;
    }
}

/* Add an operation, coalescing with current if same type */
static void add_op(diff_ctx_t* ctx, int type, int count) {
    if (type == ctx->current_type) {
        ctx->current_count += count;
    } else {
        flush_op(ctx);
        ctx->current_type = type;
        ctx->current_count = count;
    }
}

/* Callback for each line in the diff */
static int line_callback(
    const git_diff_delta* delta,
    const git_diff_hunk* hunk,
    const git_diff_line* line,
    void* payload
) {
    diff_ctx_t* ctx = (diff_ctx_t*)payload;

    switch (line->origin) {
    case GIT_DIFF_LINE_CONTEXT:
        add_op(ctx, CF_DIFF_EQUAL, 1);
        ctx->old_line_pos++;
        ctx->new_line_pos++;
        break;
    case GIT_DIFF_LINE_ADDITION:
        add_op(ctx, CF_DIFF_INSERT, 1);
        ctx->new_line_pos++;
        break;
    case GIT_DIFF_LINE_DELETION:
        add_op(ctx, CF_DIFF_DELETE, 1);
        ctx->old_line_pos++;
        break;
    default:
        /* Skip file headers, hunk headers, etc. */
        break;
    }

    return 0;
}

/* Callback for each hunk in the diff */
static int hunk_callback(
    const git_diff_delta* delta,
    const git_diff_hunk* hunk,
    void* payload
) {
    diff_ctx_t* ctx = (diff_ctx_t*)payload;

    /* Insert implicit equal block for skipped lines before this hunk */
    /* old_start is 1-based, old_line_pos is 0-based count of processed lines */
    int hunk_start = hunk->old_start - 1;  /* Convert to 0-based */
    if (hunk_start > ctx->old_line_pos) {
        int skipped = hunk_start - ctx->old_line_pos;
        add_op(ctx, CF_DIFF_EQUAL, skipped);
        ctx->old_line_pos = hunk_start;
        ctx->new_line_pos += skipped;
    }

    return 0;
}

/* Compute diff for a single blob pair */
static int compute_single_diff(
    git_repository* repo,
    const cf_diff_request* req,
    cf_diff_result* result
) {
    git_blob* old_blob = NULL;
    git_blob* new_blob = NULL;
    int ret = CF_OK;

    /* Initialize result */
    ret = cf_init_diff_result(result, CF_MAX_DIFF_OPS);
    if (ret != CF_OK) {
        return ret;
    }

    /* Load old blob if exists */
    if (req->has_old) {
        int err = git_blob_lookup(&old_blob, repo, &req->old_oid);
        if (err != 0) {
            result->error = CF_ERR_LOOKUP;
            return CF_ERR_LOOKUP;
        }

        const char* content = (const char*)git_blob_rawcontent(old_blob);
        size_t size = git_blob_rawsize(old_blob);

        /* Check for binary */
        if (size > 0 && cf_is_binary(content, size)) {
            git_blob_free(old_blob);
            result->error = CF_ERR_BINARY;
            return CF_ERR_BINARY;
        }

        result->old_lines = cf_count_lines(content, size);
    }

    /* Load new blob if exists */
    if (req->has_new) {
        int err = git_blob_lookup(&new_blob, repo, &req->new_oid);
        if (err != 0) {
            if (old_blob) git_blob_free(old_blob);
            result->error = CF_ERR_LOOKUP;
            return CF_ERR_LOOKUP;
        }

        const char* content = (const char*)git_blob_rawcontent(new_blob);
        size_t size = git_blob_rawsize(new_blob);

        /* Check for binary */
        if (size > 0 && cf_is_binary(content, size)) {
            if (old_blob) git_blob_free(old_blob);
            git_blob_free(new_blob);
            result->error = CF_ERR_BINARY;
            return CF_ERR_BINARY;
        }

        result->new_lines = cf_count_lines(content, size);
    }

    /* Setup diff context */
    diff_ctx_t ctx = {
        .result = result,
        .current_type = -1,
        .current_count = 0,
        .old_line_pos = 0,
        .new_line_pos = 0
    };

    /* Compute diff using libgit2 */
    git_diff_options opts = GIT_DIFF_OPTIONS_INIT;
    int err = git_diff_blobs(
        old_blob, NULL,
        new_blob, NULL,
        &opts,
        NULL,           /* file_cb */
        NULL,           /* binary_cb */
        hunk_callback,
        line_callback,
        &ctx
    );

    if (err != 0) {
        if (old_blob) git_blob_free(old_blob);
        if (new_blob) git_blob_free(new_blob);
        result->error = CF_ERR_DIFF;
        return CF_ERR_DIFF;
    }

    /* Flush any pending operation */
    flush_op(&ctx);

    /* Add trailing equal block for remaining unchanged lines */
    if (result->old_lines > ctx.old_line_pos) {
        int remaining = result->old_lines - ctx.old_line_pos;
        add_op(&ctx, CF_DIFF_EQUAL, remaining);
        flush_op(&ctx);
    }

    if (old_blob) git_blob_free(old_blob);
    if (new_blob) git_blob_free(new_blob);

    return CF_OK;
}

/* Structure for preloaded blob data */
typedef struct {
    git_oid oid;
    const char* data;
    size_t size;
    git_odb_object* obj;  /* Keep reference for cleanup */
    int is_binary;
    int line_count;
    int valid;
} cf_preloaded_blob;

/* OID comparison for sorting */
static int compare_oids_diff(const void* a, const void* b) {
    return memcmp(a, b, GIT_OID_RAWSZ);
}

/*
 * Preload unique blobs from a batch of diff requests.
 * Returns a map of OID -> preloaded blob data.
 */
static int preload_blobs_for_diff(
    git_odb* odb,
    const cf_diff_request* requests,
    int count,
    cf_preloaded_blob** out_blobs,
    int* out_blob_count
) {
    /* Collect all unique OIDs */
    git_oid* all_oids = (git_oid*)malloc(count * 2 * sizeof(git_oid));
    if (all_oids == NULL) {
        return CF_ERR_NOMEM;
    }

    int oid_count = 0;
    for (int i = 0; i < count; i++) {
        if (requests[i].has_old) {
            memcpy(&all_oids[oid_count++], &requests[i].old_oid, sizeof(git_oid));
        }
        if (requests[i].has_new) {
            memcpy(&all_oids[oid_count++], &requests[i].new_oid, sizeof(git_oid));
        }
    }

    if (oid_count == 0) {
        free(all_oids);
        *out_blobs = NULL;
        *out_blob_count = 0;
        return CF_OK;
    }

    /* Sort OIDs for pack locality and deduplication */
    qsort(all_oids, oid_count, sizeof(git_oid), compare_oids_diff);

    /* Count unique OIDs */
    int unique_count = 1;
    for (int i = 1; i < oid_count; i++) {
        if (memcmp(&all_oids[i], &all_oids[i-1], sizeof(git_oid)) != 0) {
            unique_count++;
        }
    }

    /* Allocate preloaded blob array */
    cf_preloaded_blob* blobs = (cf_preloaded_blob*)calloc(unique_count, sizeof(cf_preloaded_blob));
    if (blobs == NULL) {
        free(all_oids);
        return CF_ERR_NOMEM;
    }

    /* Load unique blobs in sorted order (maximizes pack cache hits) */
    int blob_idx = 0;
    git_oid prev_oid;
    memset(&prev_oid, 0, sizeof(git_oid));

    for (int i = 0; i < oid_count; i++) {
        /* Skip duplicates */
        if (i > 0 && memcmp(&all_oids[i], &prev_oid, sizeof(git_oid)) == 0) {
            continue;
        }
        memcpy(&prev_oid, &all_oids[i], sizeof(git_oid));

        cf_preloaded_blob* blob = &blobs[blob_idx];
        memcpy(&blob->oid, &all_oids[i], sizeof(git_oid));

        git_odb_object* obj = NULL;
        int err = git_odb_read(&obj, odb, &all_oids[i]);
        if (err != 0 || git_odb_object_type(obj) != GIT_OBJECT_BLOB) {
            if (obj) git_odb_object_free(obj);
            blob->valid = 0;
            blob_idx++;
            continue;
        }

        blob->obj = obj;
        blob->data = (const char*)git_odb_object_data(obj);
        blob->size = git_odb_object_size(obj);
        blob->is_binary = (blob->size > 0) ? cf_is_binary(blob->data, blob->size) : 0;
        blob->line_count = (!blob->is_binary && blob->size > 0) ? cf_count_lines(blob->data, blob->size) : 0;
        blob->valid = 1;
        blob_idx++;
    }

    free(all_oids);
    *out_blobs = blobs;
    *out_blob_count = unique_count;
    return CF_OK;
}

/* Find a preloaded blob by OID using binary search */
static cf_preloaded_blob* find_preloaded_blob(
    cf_preloaded_blob* blobs,
    int blob_count,
    const git_oid* oid
) {
    int left = 0, right = blob_count - 1;
    while (left <= right) {
        int mid = (left + right) / 2;
        int cmp = memcmp(oid->id, blobs[mid].oid.id, GIT_OID_RAWSZ);
        if (cmp == 0) {
            return &blobs[mid];
        } else if (cmp < 0) {
            right = mid - 1;
        } else {
            left = mid + 1;
        }
    }
    return NULL;
}

/* Free preloaded blobs */
static void free_preloaded_blobs(cf_preloaded_blob* blobs, int count) {
    if (blobs == NULL) return;
    for (int i = 0; i < count; i++) {
        if (blobs[i].obj != NULL) {
            git_odb_object_free(blobs[i].obj);
        }
    }
    free(blobs);
}

/* Compute diff using preloaded blobs */
static int compute_diff_with_preloaded(
    cf_preloaded_blob* old_blob,
    cf_preloaded_blob* new_blob,
    cf_diff_result* result
) {
    /* Initialize result */
    int ret = cf_init_diff_result(result, CF_MAX_DIFF_OPS);
    if (ret != CF_OK) {
        return ret;
    }

    /* Check old blob */
    if (old_blob != NULL) {
        if (!old_blob->valid) {
            result->error = CF_ERR_LOOKUP;
            return CF_ERR_LOOKUP;
        }
        if (old_blob->is_binary) {
            result->error = CF_ERR_BINARY;
            return CF_ERR_BINARY;
        }
        result->old_lines = old_blob->line_count;
    }

    /* Check new blob */
    if (new_blob != NULL) {
        if (!new_blob->valid) {
            result->error = CF_ERR_LOOKUP;
            return CF_ERR_LOOKUP;
        }
        if (new_blob->is_binary) {
            result->error = CF_ERR_BINARY;
            return CF_ERR_BINARY;
        }
        result->new_lines = new_blob->line_count;
    }

    /* Setup diff context */
    diff_ctx_t ctx = {
        .result = result,
        .current_type = -1,
        .current_count = 0,
        .old_line_pos = 0,
        .new_line_pos = 0
    };

    /* Create temporary blobs for git_diff_blobs (it needs git_blob pointers) */
    /* Unfortunately git_diff_blobs requires git_blob*, not raw data */
    /* We'll use git_blob_create_from_buffer to create in-memory blobs */
    
    /* For now, use the raw diff approach with buffers */
    git_diff_options opts = GIT_DIFF_OPTIONS_INIT;
    
    /* git_diff_buffers is more efficient when we already have the data */
    int err = git_diff_buffers(
        old_blob ? old_blob->data : NULL,
        old_blob ? old_blob->size : 0,
        NULL,  /* old_as_path */
        new_blob ? new_blob->data : NULL,
        new_blob ? new_blob->size : 0,
        NULL,  /* new_as_path */
        &opts,
        NULL,           /* file_cb */
        NULL,           /* binary_cb */
        hunk_callback,
        line_callback,
        &ctx
    );

    if (err != 0) {
        result->error = CF_ERR_DIFF;
        return CF_ERR_DIFF;
    }

    /* Flush any pending operation */
    flush_op(&ctx);

    /* Add trailing equal block for remaining unchanged lines */
    if (result->old_lines > ctx.old_line_pos) {
        int remaining = result->old_lines - ctx.old_line_pos;
        add_op(&ctx, CF_DIFF_EQUAL, remaining);
        flush_op(&ctx);
    }

    return CF_OK;
}

/*
 * Compute diffs for multiple blob pairs in a single call.
 *
 * Optimizations:
 * 1. Preloads all unique blobs in sorted order for pack cache efficiency
 * 2. Uses git_diff_buffers instead of git_diff_blobs (avoids re-lookup)
 * 3. Single ODB refresh for the entire batch
 */
int cf_batch_diff_blobs(
    git_repository* repo,
    const cf_diff_request* requests,
    int count,
    cf_diff_result* results
) {
    if (count == 0) {
        return 0;
    }

    /* Get ODB for direct access */
    git_odb* odb = NULL;
    int err = git_repository_odb(&odb, repo);
    if (err != 0) {
        /* Fall back to basic diff on ODB error */
        int success_count = 0;
        for (int i = 0; i < count; i++) {
            if (compute_single_diff(repo, &requests[i], &results[i]) == CF_OK) {
                success_count++;
            }
        }
        return success_count;
    }

    /* Refresh ODB once for the entire batch */
    git_odb_refresh(odb);

    /* Preload all blobs */
    cf_preloaded_blob* preloaded = NULL;
    int preloaded_count = 0;
    err = preload_blobs_for_diff(odb, requests, count, &preloaded, &preloaded_count);
    if (err != CF_OK) {
        git_odb_free(odb);
        /* Fall back to basic diff */
        int success_count = 0;
        for (int i = 0; i < count; i++) {
            if (compute_single_diff(repo, &requests[i], &results[i]) == CF_OK) {
                success_count++;
            }
        }
        return success_count;
    }

    /* Compute diffs using preloaded blobs */
    int success_count = 0;
    for (int i = 0; i < count; i++) {
        cf_preloaded_blob* old_blob = NULL;
        cf_preloaded_blob* new_blob = NULL;

        if (requests[i].has_old) {
            old_blob = find_preloaded_blob(preloaded, preloaded_count, &requests[i].old_oid);
        }
        if (requests[i].has_new) {
            new_blob = find_preloaded_blob(preloaded, preloaded_count, &requests[i].new_oid);
        }

        if (compute_diff_with_preloaded(old_blob, new_blob, &results[i]) == CF_OK) {
            success_count++;
        }
    }

    free_preloaded_blobs(preloaded, preloaded_count);
    git_odb_free(odb);

    return success_count;
}

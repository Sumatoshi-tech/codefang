/*
 * Codefang Git Library - Diff Operations
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

/*
 * Compute diffs for multiple blob pairs in a single call.
 */
int cf_batch_diff_blobs(
    git_repository* repo,
    const cf_diff_request* requests,
    int count,
    cf_diff_result* results
) {
    int success_count = 0;

    for (int i = 0; i < count; i++) {
        int ret = compute_single_diff(repo, &requests[i], &results[i]);
        if (ret == CF_OK) {
            success_count++;
        }
    }

    return success_count;
}

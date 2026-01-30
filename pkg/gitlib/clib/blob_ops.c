/*
 * Codefang Git Library - Blob Operations
 */

#include "codefang_git.h"
#include <stdlib.h>
#include <string.h>

/*
 * Load multiple blobs in a single call.
 *
 * This function loads multiple blobs from the repository, copying the data
 * to malloc'd memory that the caller must free.
 */
int cf_batch_load_blobs(
    git_repository* repo,
    const cf_blob_request* requests,
    int count,
    cf_blob_result* results
) {
    int success_count = 0;

    for (int i = 0; i < count; i++) {
        cf_blob_result* res = &results[i];

        /* Initialize result */
        memcpy(res->oid, requests[i].oid.id, 20);
        res->data = NULL;
        res->size = 0;
        res->error = CF_OK;
        res->is_binary = 0;
        res->line_count = 0;

        /* Lookup the blob */
        git_blob* blob = NULL;
        int err = git_blob_lookup(&blob, repo, &requests[i].oid);
        if (err != 0) {
            res->error = CF_ERR_LOOKUP;
            continue;
        }

        /* Get blob content and size */
        const void* content = git_blob_rawcontent(blob);
        size_t size = git_blob_rawsize(blob);

        /* Check for binary content */
        if (size > 0) {
            res->is_binary = cf_is_binary((const char*)content, size);
        }

        /* Count lines if not binary */
        if (!res->is_binary && size > 0) {
            res->line_count = cf_count_lines((const char*)content, size);
        }

        /* Copy data to malloc'd memory */
        if (size > 0) {
            res->data = malloc(size);
            if (res->data == NULL) {
                git_blob_free(blob);
                res->error = CF_ERR_NOMEM;
                continue;
            }
            memcpy(res->data, content, size);
        }

        res->size = size;
        git_blob_free(blob);
        success_count++;
    }

    return success_count;
}

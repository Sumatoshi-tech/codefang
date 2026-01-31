/*
 * Codefang Git Library - Blob Operations
 *
 * Optimized for batch loading with pack-aware strategies:
 * 1. Uses ODB API directly for faster raw object access
 * 2. Pre-validates OIDs in batch to reduce redundant lookups
 * 3. Sorts OIDs for better pack cache locality
 * 4. OpenMP parallel loading (git_odb is thread-safe for reading)
 */

#include "codefang_git.h"
#include <stdlib.h>
#include <string.h>

#ifdef _OPENMP
#include <omp.h>
#endif

/* Structure for sorting OIDs while tracking original indices */
typedef struct {
    git_oid oid;
    int original_index;
} cf_oid_with_index;

/* Comparison function for sorting OIDs (improves pack cache locality) */
static int compare_oids(const void* a, const void* b) {
    const cf_oid_with_index* oid_a = (const cf_oid_with_index*)a;
    const cf_oid_with_index* oid_b = (const cf_oid_with_index*)b;
    return memcmp(oid_a->oid.id, oid_b->oid.id, GIT_OID_RAWSZ);
}

/*
 * Load a single blob using ODB API (faster than git_blob_lookup).
 */
static int load_single_blob_odb(
    git_odb* odb,
    const git_oid* oid,
    cf_blob_result* res
) {
    git_odb_object* obj = NULL;

    /* Use ODB directly - faster than git_blob_lookup */
    int err = git_odb_read(&obj, odb, oid);
    if (err != 0) {
        res->error = CF_ERR_LOOKUP;
        return CF_ERR_LOOKUP;
    }

    /* Verify it's a blob */
    if (git_odb_object_type(obj) != GIT_OBJECT_BLOB) {
        git_odb_object_free(obj);
        res->error = CF_ERR_LOOKUP;
        return CF_ERR_LOOKUP;
    }

    /* Get content and size */
    const void* content = git_odb_object_data(obj);
    size_t size = git_odb_object_size(obj);

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
            git_odb_object_free(obj);
            res->error = CF_ERR_NOMEM;
            return CF_ERR_NOMEM;
        }
        memcpy(res->data, content, size);
    }

    res->size = size;
    git_odb_object_free(obj);

    return CF_OK;
}

/*
 * Load multiple blobs in a single call with pack-aware optimizations.
 *
 * Optimizations:
 * 1. Uses ODB API directly (git_odb_read) instead of git_blob_lookup
 * 2. Sorts OIDs to improve pack cache locality
 * 3. Refreshes ODB once before batch to ensure consistent state
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
    if (count == 0) {
        return 0;
    }

    /* Get the ODB for direct object access */
    git_odb* odb = NULL;
    int err = git_repository_odb(&odb, repo);
    if (err != 0) {
        /* Fall back to basic loading on ODB error */
        for (int i = 0; i < count; i++) {
            results[i].error = CF_ERR_LOOKUP;
        }
        return 0;
    }

    /* Refresh ODB once for the entire batch */
    git_odb_refresh(odb);

    /* Initialize all results */
    for (int i = 0; i < count; i++) {
        cf_blob_result* res = &results[i];
        memcpy(res->oid, requests[i].oid.id, GIT_OID_RAWSZ);
        res->data = NULL;
        res->size = 0;
        res->error = CF_OK;
        res->is_binary = 0;
        res->line_count = 0;
    }

    int success_count = 0;

    /* For small batches, skip sorting overhead */
    if (count <= 4) {
        for (int i = 0; i < count; i++) {
            if (load_single_blob_odb(odb, &requests[i].oid, &results[i]) == CF_OK) {
                success_count++;
            }
        }
        git_odb_free(odb);
        return success_count;
    }

    /* Create sorted index for better pack cache locality */
    cf_oid_with_index* sorted = (cf_oid_with_index*)malloc(count * sizeof(cf_oid_with_index));
    if (sorted == NULL) {
        /* Fall back to unsorted loading on allocation failure */
        for (int i = 0; i < count; i++) {
            if (load_single_blob_odb(odb, &requests[i].oid, &results[i]) == CF_OK) {
                success_count++;
            }
        }
        git_odb_free(odb);
        return success_count;
    }

    /* Copy OIDs with original indices */
    for (int i = 0; i < count; i++) {
        memcpy(&sorted[i].oid, &requests[i].oid, sizeof(git_oid));
        sorted[i].original_index = i;
    }

    /* Sort OIDs for better pack cache locality */
    qsort(sorted, count, sizeof(cf_oid_with_index), compare_oids);

    /* Load blobs in sorted order - parallelized with OpenMP.
     * git_odb is thread-safe for reading, each thread writes to its own results slot.
     */
#ifdef _OPENMP
    if (count >= 8) {
        #pragma omp parallel for reduction(+:success_count) schedule(dynamic, 4)
        for (int i = 0; i < count; i++) {
            int orig_idx = sorted[i].original_index;
            if (load_single_blob_odb(odb, &sorted[i].oid, &results[orig_idx]) == CF_OK) {
                success_count++;
            }
        }
    } else
#endif
    {
        for (int i = 0; i < count; i++) {
            int orig_idx = sorted[i].original_index;
            if (load_single_blob_odb(odb, &sorted[i].oid, &results[orig_idx]) == CF_OK) {
                success_count++;
            }
        }
    }

    free(sorted);
    git_odb_free(odb);

    return success_count;
}

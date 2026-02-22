/*
 * Codefang Git Library - Utility Functions
 */

#include "codefang_git.h"
#include <stdlib.h>
#include <string.h>
#include <malloc.h>

#ifdef _OPENMP
#include <omp.h>
#endif

/*
 * Initialize global settings.
 * Should be called once at startup.
 */
void cf_init() {
#ifdef _OPENMP
    // Disable nested parallelism by default to avoid thread oversubscription
    // when running inside Go goroutines.
    omp_set_num_threads(1);
#endif
}

/*
 * Configure libgit2 global memory limits.
 *
 * mwindow_mapped_limit: maximum bytes of pack file data to mmap at once.
 *   Default is 8 GiB on 64-bit, which can dominate RSS on large repos.
 * cache_max_size: maximum bytes for the global object cache (decompressed
 *   objects like commits, trees, blobs). Default is 256 MiB.
 *
 * Both are global settings shared across all repository handles.
 * Must be called before opening repositories for full effect.
 */
int cf_configure_memory(size_t mwindow_mapped_limit, size_t cache_max_size, int malloc_arena_max) {
    int err = 0;
    if (mwindow_mapped_limit > 0) {
        err = git_libgit2_opts(GIT_OPT_SET_MWINDOW_MAPPED_LIMIT, mwindow_mapped_limit);
        if (err != 0) return err;
    }
    if (cache_max_size > 0) {
        err = git_libgit2_opts(GIT_OPT_SET_CACHE_MAX_SIZE, (ssize_t)cache_max_size);
        if (err != 0) return err;
    }
    /* Limit glibc malloc arenas to prevent RSS explosion.
     * Default is 8*num_cores which creates ~192 arenas on 24-core machines.
     * Each arena retains freed memory, causing RSS to be 3-4x higher than
     * actual usage. A moderate limit (e.g. 4-8) dramatically reduces peak
     * RSS with minimal performance impact. */
    if (malloc_arena_max > 0) {
        mallopt(M_ARENA_MAX, malloc_arena_max);
    }
    return 0;
}

/*
 * Count lines in a buffer.
 *
 * Matches Go's CountLines behavior:
 * - Count number of '\n'
 * - If last byte is not '\n', add 1
 * - Empty buffer is 0 lines
 */
int cf_count_lines(const char* data, size_t size) {
    if (size == 0) {
        return 0;
    }

    int count = 0;
    const char* ptr = data;
    const char* end = data + size;

    while (ptr < end) {
        // Optimized search for newline
        const char* newline = memchr(ptr, '\n', end - ptr);
        if (newline == NULL) {
            break;
        }
        count++;
        ptr = newline + 1;
    }

    /* If file doesn't end with newline, the last segment is a line too */
    if (size > 0 && data[size - 1] != '\n') {
        count++;
    }

    return count;
}

/*
 * Check if data appears to be binary.
 *
 * Checks for null bytes in the first CF_BINARY_CHECK_LEN bytes.
 */
int cf_is_binary(const char* data, size_t size) {
    size_t check_len = size < CF_BINARY_CHECK_LEN ? size : CF_BINARY_CHECK_LEN;

    if (check_len == 0) {
        return 0;
    }

    // Quick check for common null byte binary indicator
    const char* ptr = data;
    const char* end = data + check_len;
    while (ptr < end) {
        if (*ptr == '\0') {
            return 1;
        }
        ptr++;
    }

    return 0;
}

/*
 * Free blob result data.
 */
void cf_free_blob_results(cf_blob_result* results, int count) {
    for (int i = 0; i < count; i++) {
        if (results[i].data != NULL) {
            free(results[i].data);
            results[i].data = NULL;
        }
    }
}

/*
 * Free diff result data.
 */
void cf_free_diff_results(cf_diff_result* results, int count) {
    for (int i = 0; i < count; i++) {
        if (results[i].ops != NULL) {
            free(results[i].ops);
            results[i].ops = NULL;
        }
    }
}

/*
 * Initialize a diff result with pre-allocated ops array.
 */
int cf_init_diff_result(cf_diff_result* result, int capacity) {
    result->old_lines = 0;
    result->new_lines = 0;
    result->op_count = 0;
    result->op_capacity = capacity;
    result->error = 0;

    result->ops = (cf_diff_op*)malloc(capacity * sizeof(cf_diff_op));
    if (result->ops == NULL) {
        result->error = CF_ERR_NOMEM;
        return CF_ERR_NOMEM;
    }

    return CF_OK;
}

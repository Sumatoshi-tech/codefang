/*
 * Codefang Git Library - Utility Functions
 */

#include "codefang_git.h"
#include <stdlib.h>
#include <string.h>

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

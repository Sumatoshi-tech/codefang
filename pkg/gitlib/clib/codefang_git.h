/*
 * Codefang Git Library - C Core Operations
 *
 * This library provides optimized batch operations for Git data processing,
 * designed to minimize CGO overhead by processing multiple items per call.
 */

#ifndef CODEFANG_GIT_H
#define CODEFANG_GIT_H

#include <git2.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ============================================================================
 * Constants
 * ============================================================================ */

/* Maximum operations per diff result (for pre-allocation) */
#define CF_MAX_DIFF_OPS 100000

/* Number of bytes to check for binary detection */
#define CF_BINARY_CHECK_LEN 8000

/* Diff operation types */
#define CF_DIFF_EQUAL  0
#define CF_DIFF_INSERT 1
#define CF_DIFF_DELETE 2

/* Error codes */
#define CF_OK           0
#define CF_ERR_NOMEM   -1
#define CF_ERR_BINARY  -2
#define CF_ERR_LOOKUP  -3
#define CF_ERR_DIFF    -4

/* ============================================================================
 * Blob Operations Types
 * ============================================================================ */

/* Result of loading a single blob */
typedef struct {
    unsigned char oid[20];  /* The blob's OID */
    void* data;             /* Pointer to blob data (malloc'd, caller must free) */
    size_t size;            /* Size of the blob data */
    int error;              /* 0 on success, negative on error */
    int is_binary;          /* 1 if binary content detected */
    int line_count;         /* Number of lines (0 if binary) */
} cf_blob_result;

/* Request for batch blob loading */
typedef struct {
    git_oid oid;            /* The blob OID to load */
} cf_blob_request;

/* ============================================================================
 * Diff Operations Types
 * ============================================================================ */

/* Single diff operation (equal, insert, or delete) */
typedef struct {
    int type_;              /* CF_DIFF_EQUAL, CF_DIFF_INSERT, or CF_DIFF_DELETE */
    int line_count;         /* Number of lines affected */
} cf_diff_op;

/* Result of diffing two blobs */
typedef struct {
    int old_lines;          /* Total lines in old blob */
    int new_lines;          /* Total lines in new blob */
    cf_diff_op* ops;        /* Array of diff operations (malloc'd, caller must free) */
    int op_count;           /* Number of operations in ops array */
    int op_capacity;        /* Capacity of ops array */
    int error;              /* 0 on success, negative on error */
} cf_diff_result;

/* Request for diffing two blobs */
typedef struct {
    git_oid old_oid;        /* OID of old blob (zero OID if new file) */
    git_oid new_oid;        /* OID of new blob (zero OID if deleted) */
    const void* old_data;   /* Optional: pointer to old blob data (if already loaded) */
    size_t old_size;        /* Size of old data */
    const void* new_data;   /* Optional: pointer to new blob data (if already loaded) */
    size_t new_size;        /* Size of new data */
    int has_old;            /* 1 if old_oid is valid */
    int has_new;            /* 1 if new_oid is valid */
} cf_diff_request;

/* ============================================================================
 * Batch Operations - Core API
 * ============================================================================ */

/*
 * Load multiple blobs in a single call.
 *
 * This function loads multiple blobs from the repository, minimizing CGO
 * overhead by processing all requests in a single call.
 *
 * @param repo     The git repository
 * @param requests Array of blob requests (OIDs to load)
 * @param count    Number of requests
 * @param results  Pre-allocated array to store results (must have 'count' elements)
 * @return         Number of successfully loaded blobs, or negative on fatal error
 */
int cf_batch_load_blobs(
    git_repository* repo,
    const cf_blob_request* requests,
    int count,
    cf_blob_result* results
);

/*
 * Compute diffs for multiple blob pairs in a single call.
 *
 * This function computes line-level diffs for multiple blob pairs,
 * minimizing CGO overhead by processing all requests in a single call.
 *
 * @param repo     The git repository
 * @param requests Array of diff requests (blob pairs)
 * @param count    Number of requests
 * @param results  Pre-allocated array to store results (must have 'count' elements)
 * @return         Number of successfully computed diffs, or negative on fatal error
 */
int cf_batch_diff_blobs(
    git_repository* repo,
    const cf_diff_request* requests,
    int count,
    cf_diff_result* results
);

/* ============================================================================
 * Utility Functions
 * ============================================================================ */

/*
 * Count lines in a buffer.
 *
 * Matches Go's strings.Split behavior:
 * - "a\nb\n" -> 2 lines
 * - "a\nb" -> 2 lines
 * - "" -> 0 lines
 *
 * @param data  The data buffer
 * @param size  Size of the buffer
 * @return      Number of lines
 */
int cf_count_lines(const char* data, size_t size);

/*
 * Check if data appears to be binary.
 *
 * Checks for null bytes in the first CF_BINARY_CHECK_LEN bytes.
 *
 * @param data  The data buffer
 * @param size  Size of the buffer
 * @return      1 if binary, 0 if text
 */
int cf_is_binary(const char* data, size_t size);

/* ============================================================================
 * Memory Management
 * ============================================================================ */

/*
 * Free blob result data.
 *
 * Frees the data pointer in each result.
 *
 * @param results  Array of blob results
 * @param count    Number of results
 */
void cf_free_blob_results(cf_blob_result* results, int count);

/*
 * Free diff result data.
 *
 * Frees the ops array in each result.
 *
 * @param results  Array of diff results
 * @param count    Number of results
 */
void cf_free_diff_results(cf_diff_result* results, int count);

/*
 * Initialize a diff result with pre-allocated ops array.
 *
 * @param result    The result to initialize
 * @param capacity  Initial capacity for ops array
 * @return          0 on success, CF_ERR_NOMEM on allocation failure
 */
int cf_init_diff_result(cf_diff_result* result, int capacity);

#ifdef __cplusplus
}
#endif

#endif /* CODEFANG_GIT_H */

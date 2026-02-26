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
#define CF_ERR_ARENA_FULL -5

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

/* Result of loading a blob into an arena (provided or allocated) */
typedef struct {
    unsigned char oid[20];  /* The blob's OID */
    uint64_t offset;        /* Offset in the arena */
    size_t size;            /* Size of the blob data */
    int error;              /* 0 on success, negative on error, CF_ERR_ARENA_FULL if didn't fit */
    int is_binary;          /* 1 if binary content detected */
    int line_count;         /* Number of lines (0 if binary) */
} cf_blob_arena_result;

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
 * Tree Diff Operations Types
 * ============================================================================ */

/* Single file change (equivalent to git_diff_delta) */
typedef struct {
    int status;             /* GIT_DELTA_ADDED, DELETED, MODIFIED, etc. */
    char* old_path;         /* Path of old file (malloc'd) */
    unsigned char old_oid[20];
    size_t old_size;
    uint16_t old_mode;
    
    char* new_path;         /* Path of new file (malloc'd) */
    unsigned char new_oid[20];
    size_t new_size;
    uint16_t new_mode;
} cf_change;

/* Result of tree diff */
typedef struct {
    cf_change* changes;     /* Array of changes (malloc'd) */
    int count;              /* Number of changes */
    int capacity;           /* Capacity of changes array */
    int error;              /* 0 on success */
} cf_tree_diff_result;

/*
 * Compute diff between two trees.
 * Returns a compact array of changes.
 */
int cf_tree_diff(
    git_repository* repo,
    git_oid* old_tree_oid,
    git_oid* new_tree_oid,
    cf_tree_diff_result* result
);

/*
 * Free tree diff result.
 */
void cf_free_tree_diff_result(cf_tree_diff_result* result);

/* ============================================================================
 * Batch Operations - Core API
 * ============================================================================ */

/*
 * Load multiple blobs in a single call.
 */
int cf_batch_load_blobs(
    git_repository* repo,
    const cf_blob_request* requests,
    int count,
    cf_blob_result* results
);

/*
 * Load multiple blobs into a provided memory arena.
 */
int cf_batch_load_blobs_arena(
    git_repository* repo,
    const cf_blob_request* requests,
    int count,
    void* arena_start,
    size_t arena_capacity,
    cf_blob_arena_result* results
);

/*
 * Load multiple blobs into a single C-allocated buffer (flat).
 * 
 * This function calculates total size, allocates ONE buffer, copies all blobs,
 * and returns the buffer pointer. The caller is responsible for freeing out_arena.
 *
 * @param repo           The git repository
 * @param requests       Array of blob requests
 * @param count          Number of requests
 * @param out_arena      Output: Pointer to the allocated buffer
 * @param out_arena_size Output: Size of the allocated buffer
 * @param results        Pre-allocated array to store results
 * @return               Number of successfully loaded blobs
 */
int cf_batch_load_blobs_flat(
    git_repository* repo,
    const cf_blob_request* requests,
    int count,
    void** out_arena,
    size_t* out_arena_size,
    cf_blob_arena_result* results
);

/*
 * Compute diffs for multiple blob pairs in a single call.
 */
int cf_batch_diff_blobs(
    git_repository* repo,
    const cf_diff_request* requests,
    int count,
    cf_diff_result* results
);

/* ============================================================================
 * Initialization
 * ============================================================================ */

/*
 * Initialize global library settings.
 * Should be called once at startup.
 */
void cf_init();

/*
 * Configure libgit2 global memory limits and glibc malloc arenas.
 * mwindow_mapped_limit: max bytes of mmap'd pack data (0 = no change).
 * cache_max_size: max bytes for object cache (0 = no change).
 * malloc_arena_max: max glibc malloc arenas (0 = no change).
 * Returns 0 on success.
 */
int cf_configure_memory(size_t mwindow_mapped_limit, size_t cache_max_size, int malloc_arena_max);

/* ============================================================================
 * Utility Functions
 * ============================================================================ */

int cf_count_lines(const char* data, size_t size);
int cf_is_binary(const char* data, size_t size);

/* ============================================================================
 * Memory Management
 * ============================================================================ */

void cf_free_blob_results(cf_blob_result* results, int count);
void cf_free_diff_results(cf_diff_result* results, int count);
int cf_init_diff_result(cf_diff_result* result, int capacity);
int cf_release_native_memory(void);

#ifdef __cplusplus
}
#endif

#endif /* CODEFANG_GIT_H */

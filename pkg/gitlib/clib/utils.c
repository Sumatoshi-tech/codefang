/*
 * Codefang Git Library - Utility Functions
 */

#include "codefang_git.h"
#include <stdlib.h>
#include <string.h>
#ifdef __GLIBC__
#include <malloc.h>
#endif

#ifdef _OPENMP
#include <omp.h>
#endif

/*
 * C constructor: runs before main() and before the Go runtime creates threads.
 * This is the ONLY reliable way to limit glibc malloc arenas — mallopt() after
 * threads exist cannot destroy already-created arenas.
 *
 * Without this, 24-core machines create 69 threads × 128 MiB arenas = 8+ GiB
 * of retained-but-freed native memory that RSS never reclaims.
 */
#ifdef __GLIBC__
__attribute__((constructor))
static void cf_early_malloc_config(void) {
    /* Limit to 2 arenas. With Go's M:N scheduling model, dozens of OS threads
     * are created for CGO calls. Each thread that calls malloc for the first
     * time creates a new arena (up to 8*cores = 192 on 24-core). Capping at 2
     * forces all threads to share 2 arenas, preventing RSS explosion.
     * Contention on arena locks is acceptable because the hot CGO paths
     * (tree-sitter, libgit2) do bulk work per call. */
    mallopt(M_ARENA_MAX, 2);

    /* Force allocations >= 32 KiB to use mmap instead of sbrk/arena.
     * Tree-sitter parse trees are typically 100 KiB - 10 MiB; using mmap
     * ensures they are returned to the OS immediately on free(). */
    mallopt(M_MMAP_THRESHOLD, 32 * 1024);

    /* Lower trim threshold so malloc_trim releases small gaps sooner. */
    mallopt(M_TRIM_THRESHOLD, 16 * 1024);
}
#endif

/*
 * Initialize global settings.
 * Should be called once at startup, before any libgit2/tree-sitter work.
 *
 * NOTE: mallopt calls are NOT made here — they are too late. By the time Go's
 * init() calls cf_init(), the Go runtime has already created ~69 OS threads,
 * each of which may have initialized a malloc arena. glibc malloc tunables
 * must be set via environment variables (MALLOC_ARENA_MAX, MALLOC_MMAP_THRESHOLD_,
 * etc.) BEFORE the process starts. See ensureMallocTunables() in main.go.
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
    (void)malloc_arena_max; /* mallopt is handled via env vars before process start. */
    if (mwindow_mapped_limit > 0) {
        err = git_libgit2_opts(GIT_OPT_SET_MWINDOW_MAPPED_LIMIT, mwindow_mapped_limit);
        if (err != 0) return err;
    }
    if (cache_max_size > 0) {
        err = git_libgit2_opts(GIT_OPT_SET_CACHE_MAX_SIZE, (ssize_t)cache_max_size);
        if (err != 0) return err;
    }
    return 0;
}

/*
 * Release native memory back to the OS.
 *
 * On glibc, calls malloc_trim(0) which returns all free pages from
 * malloc arenas to the operating system. This is the native-side
 * counterpart to Go's debug.FreeOSMemory(). Should be called between
 * streaming chunks after bulk free() cycles from libgit2 operations.
 *
 * Returns 1 if memory was actually returned to the OS, 0 otherwise.
 */
int cf_release_native_memory(void) {
#ifdef __GLIBC__
    return malloc_trim(0);
#else
    return 0;
#endif
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

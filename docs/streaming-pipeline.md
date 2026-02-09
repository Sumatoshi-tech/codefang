# Streaming Pipeline

The streaming pipeline processes large Git histories in bounded-memory chunks.
When a repository has more commits than fit in a single memory budget window,
the pipeline splits them into chunks and processes each sequentially with
hibernate/boot cycles between chunks.

## Chunk Planning

The `streaming.Planner` determines chunk boundaries based on the memory budget:

- **MinChunkSize**: 200 commits (amortize hibernation overhead).
- **MaxChunkSize**: 500 commits (bound peak memory growth).
- **BaseOverhead**: 400 MiB (Go runtime + libgit2 + caches).
- **AvgStateGrowthPerCommit**: 500 KiB (burndown + couples + shotness state).

Chunk size = `min(MaxChunkSize, max(MinChunkSize, (budget - overhead) / growthPerCommit))`.

## Double-Buffered Chunk Pipelining

When the memory budget is sufficient and the workload requires multiple chunks,
the streaming pipeline automatically enables **double-buffered chunk pipelining**.
This overlaps chunk K+1's Git pipeline (blob loading, diffing, UAST parsing)
with chunk K's analyzer consumption.

### How It Works

```
Time -->

Chunk 1:  [Pipeline]  [Consume]
Chunk 2:              [Pipeline]  [Consume]
Chunk 3:                          [Pipeline]  [Consume]
                                      ^            ^
                            overlap with       overlap with
                            chunk 1 consume    chunk 2 consume
```

1. **Chunk 1** runs normally: pipeline then consume.
2. While chunk 1's analyzers are consuming, chunk 2's pipeline starts in a
   background goroutine (with its own repo handle and Coordinator).
3. When chunk 1's consumption finishes, chunk 2's pipeline data is already
   collected. The analyzers hibernate/boot and then consume the pre-fetched data
   directly (skipping a second Coordinator creation).
4. While chunk 2's pre-fetched data is being consumed, chunk 3's pipeline starts.

### Memory Budget Split

When double-buffering is active, the memory budget is halved to accommodate
two concurrent chunks:

```
perChunkBudget = (totalBudget - BaseOverhead) / 2 + BaseOverhead
```

This may produce more (smaller) chunks than sequential processing, but the
pipeline overlap compensates for the extra hibernation cycles.

### Activation Conditions

Double-buffering activates automatically when:

- The workload requires **2 or more chunks**.
- The available memory (budget minus overhead) is at least
  `2 * MinChunkSize * AvgStateGrowthPerCommit` (enough for two minimal chunks).

When conditions are not met, the pipeline falls back to sequential chunk
processing with no overhead.

### Isolation

Each pre-fetch Coordinator opens its own repository handle and worker pool.
There is no shared mutable state between the current chunk's Coordinator and
the pre-fetch Coordinator. This ensures thread safety without locks.

### Error Handling

- If the current chunk's consumption fails, any pending pre-fetch goroutine
  is drained (waited on) before returning the error, preventing goroutine leaks.
- If the pre-fetch pipeline encounters an error, it is surfaced when the
  pre-fetched chunk would be consumed.

### Observability

Log messages distinguish double-buffered processing:

```
streaming: processing 56000 commits in 224 chunks (double-buffer=true)
streaming[db]: processing chunk 1/224 (commits 0-250)
streaming[db]: consuming prefetched chunk 2/224 (commits 250-500)
```

## Checkpointing

Checkpoints are saved after each fully consumed chunk (except the last one).
In double-buffered mode, checkpoints are saved after both the normally-processed
chunk and the pre-fetched chunk, maintaining the same consistency guarantees
as sequential processing.

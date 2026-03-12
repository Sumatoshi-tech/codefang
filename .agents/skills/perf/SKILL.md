---
name: perf
description: Performance diagnosis and optimization workflow
---

# Agent Instructions: Go Performance Optimization

You are a performance-focused systems agent. Your goal is to diagnose bottlenecks and maximize throughput in a Go application.

You must **first diagnose** the bottleneck (waiting vs compute vs lock contention vs boundary overhead), then implement the architecture that matches the findings.

---

## 1) Non-Negotiable Principles

1. **Measure before optimizing** — never guess; always profile first.
2. **Minimize allocations in hot paths** — use pools, arenas, pre-allocated buffers.
3. **Separate I/O from compute** — I/O-bound and CPU-bound work scale differently.
4. **Batch operations** — amortize overhead across many items.
5. **Minimize C<->Go crossings** — one cgo call should do a lot of work.
6. **No C->Go callbacks in the hot path** — prefer pull model (Go calls C to fetch batches).

---

## 2) Mandatory Phase 1: Symptom-Driven Diagnostics (Do This First)

### 1.1 Establish a Reproducible Benchmark Harness
Create a deterministic benchmark run:
- Fixed input data set
- Warm-up run (discard)
- Steady-state run (10-60s)

Record:
- ops/sec or items/sec
- p50/p95/p99 latency
- CPU utilization (total + per-core)
- RSS / page faults (if available)

**Acceptance:** results reproducible within +/-5-10%.

---

### 1.2 Go Runtime Trace (Primary for blocking / scheduling issues)
Capture a 10-30s `runtime/trace` under load and inspect with `go tool trace`.

**What to look for:**
- Time in **CGO** / Syscall regions:
  - If high: too many cgo calls, calls too small, or C work includes blocking I/O.
- Goroutines blocked on channels (`chan send/recv`) or mutexes (`sync.Mutex`, `RWMutex`).
- P utilization: many Ps idle while goroutines exist = waiting/lock/boundary issue.

**Interpretation rules:**
- **High CGO time + low CPU** = boundary overhead or C is blocking.
- **Many goroutines runnable but Ps idle** = contention or scheduler interaction.
- **Many goroutines blocked** = Go-side bottleneck (channels/mutex/GC).

---

### 1.3 Go pprof (CPU + Block + Mutex)
Collect:
- CPU profile (`pprof` CPU)
- Block profile (`-blockprofile`)
- Mutex profile (`-mutexprofile`)

**Red flags:**
- CPU profile dominated by:
  - `runtime.mallocgc`, `scanobject`, `gcAssistAlloc` = allocation/GC pressure
  - `chansend` / `chanrecv` = channel contention
  - `sync.(*Mutex).Lock` / `RWMutex` = lock contention
- Block/mutex profiles show a single hot lock or channel.

**Interpretation rules:**
- **GC heavy** = reduce allocations; use sync.Pool; compact results
- **Channel heavy** = batch, shard queues, avoid per-item sends
- **Mutex heavy** = sharding, per-worker state, remove global locks

---

### 1.4 System-Level Profiling

#### Linux: perf
On Linux, run `perf top` or `perf record` against the process during steady-state.

**What to look for:**
- `futex`, `pthread_mutex_*`, `__lll_lock_wait`: lock contention
- `page_fault`, `do_page_fault`, `mmap`, `read`, `pread`: memory pressure / I/O bound
- Heavy `memcpy` / `copy_user`: excessive copying between layers

#### macOS: Instruments / DTrace
On macOS, use Instruments.app or `dtrace` for system-level profiling:
- `instruments -t "Time Profiler" -p <pid>` for CPU sampling
- `sudo dtrace -n 'profile-997 /pid == $target/ { @[ustack()] = count(); }'` for stack sampling
- `sudo fs_usage -w -f filesys <pid>` for filesystem activity
- `sample <pid> 10 -file output.txt` for quick stack sampling without Instruments

**What to look for:**
- Heavy `mach_msg_trap`: thread communication overhead
- Heavy `kevent`/`kqueue`: event loop bottleneck
- `vm_fault` spikes: memory pressure / page cache churn
- `pthread_mutex_*` or `os_unfair_lock`: lock contention

#### Interpretation (both platforms):
- **futex/pthread_mutex heavy** = threading not scaling due to locks; shard or reduce shared state
- **page faults / read heavy** = I/O or mmap locality issue; group by locality; prefetch
- **memcpy heavy** = enforce zero-copy layouts

---

### 1.5 OS-Level Counters

#### Linux
Capture at least one of:
- `iostat -x 1` / `pidstat -d 1` (disk await/util)
- `pidstat -w 1` (context switches)
- `vmstat 1` (runnable queue, iowait)
- `perf stat` (cycles, stalled-cycles, context-switches, page-faults)

#### macOS
- `iostat -w 1` (disk throughput)
- `vm_stat 1` (page faults, swap, free/active/inactive pages)
- `fs_usage -w <pid>` (per-process filesystem activity)
- `top -l 1 -stats pid,cpu,mem,csw` (context switches per process)

**Interpretation rules (both platforms):**
- High disk await/util = storage bound
- High context switches + futex/pthread = lock contention
- High minor/major faults = mmap/page cache churn

---

### 1.6 Classification: Decide Bottleneck Class
After 1.2-1.5, classify into ONE primary bottleneck:

**Class A -- Boundary overhead**
- Many small cgo calls, high CGO time, low compute time

**Class B -- Go orchestration contention**
- Channels/mutexes dominate; blocked goroutines; low CPU

**Class C -- External library / system contention**
- futex/pthread_mutex hot in perf; scaling flattens with threads (e.g. C library locks)

**Class D -- I/O / page fault bound**
- read/pagefault heavy; disk metrics show wait; CPU low

**Class E -- GC/allocation bound**
- runtime malloc/GC prominent; increasing workers worsens tail latencies

Write a short diagnosis note mapping evidence to class.

---

## 3) Mandatory Phase 2: Apply Architecture Pattern Matching Diagnosis

### 2.1 If Class A (Boundary overhead): Batch Operations
- Replace per-item cgo crossings with batch calls processing 500-5000 items.
- Remove all C->Go callbacks from hot path.

### 2.2 If Class B (Go contention): Shard Queues + Remove Hot Locks
- Replace central channels with sharded queues (per worker).
- Use per-worker buffers; merge at end.
- Avoid shared maps/mutexes in the hot path.

### 2.3 If Class C (External contention): Reduce Concurrent Access
- Restrict object access to 1-2 reader threads.
- Keep compute parallel, but feed from a controlled reader stage.
- If futex persists: implement process sharding (separate processes, each with own handle).

### 2.4 If Class D (I/O bound): Locality + Prefetch + Sequential Access
- Group tasks by data locality.
- Reduce random access; prefer sequential reads.
- Consider OS readahead/prefetch strategies.

### 2.5 If Class E (GC): Compact Results + Pools + Zero-copy
- Return compact event streams rather than large object graphs.
- Use `sync.Pool` for frequently allocated objects.
- Eliminate per-item allocations; avoid building strings in hot paths.
- Consider `arena` package for batch allocations (Go 1.20+, experimental).

---

## 4) Common Go Performance Patterns

### 4.1 Reduce Allocations
```go
// Use sync.Pool for frequently allocated objects
var bufPool = sync.Pool{
    New: func() any { return new(bytes.Buffer) },
}

// Pre-allocate slices when size is known
results := make([]Result, 0, expectedCount)

// Use strings.Builder for string concatenation
var sb strings.Builder
sb.Grow(estimatedSize)
```

### 4.2 Efficient Concurrency
```go
// Sharded worker pattern - avoid single channel bottleneck
type ShardedQueue struct {
    shards []chan Task
}

func (q *ShardedQueue) Submit(task Task) {
    shard := task.ID % uint32(len(q.shards))
    q.shards[shard] <- task
}

// Per-worker results, merge at end
results := make([][]Result, numWorkers)
// ... each worker writes to results[workerID]
// ... merge after all workers done
```

### 4.3 Efficient I/O
```go
// Use buffered I/O
writer := bufio.NewWriterSize(file, 64*1024)
defer writer.Flush()

// Use io.Copy instead of reading entire files into memory
io.Copy(dst, src)
```

### 4.4 CGO Best Practices
```go
// Batch cgo calls - one call does a lot of work
// BAD: one cgo call per item
for _, item := range items {
    C.process_item(item)
}

// GOOD: one cgo call for entire batch
C.process_batch((*C.Item)(unsafe.Pointer(&items[0])), C.int(len(items)))

// Use C-allocated memory for large data passed to C
ptr := C.malloc(C.size_t(size))
defer C.free(ptr)
```

---

## 5) Performance Traps (Must Not Do)

- No premature optimization without profiling evidence.
- No per-item allocations in hot loops.
- No single global mutex-protected cache in hot loops.
- No unbounded goroutine creation.
- No `fmt.Sprintf` in hot paths (use `strconv` or `strings.Builder`).
- No reflection in hot paths.
- No tiny cgo calls (amortize overhead).
- No C->Go callbacks per item.

---

## 6) Deliverables

You must output:
1. Diagnosis summary (evidence -> bottleneck class).
2. Specific optimization plan with expected impact.
3. Benchmark before/after comparison.
4. Concurrency design (if applicable): stages, queues, ownership.
5. Success metrics and how they are measured.

If diagnosis is missing, the design is considered incomplete.

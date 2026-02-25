#!/usr/bin/env python3
"""
Re-run key benchmarks with the optimized uast/codefang build.
Merges results into kubernetes_benchmark_results.json with _v2 suffix.
"""

import json
import os
import re
import subprocess
import time
from dataclasses import dataclass, field, asdict
from datetime import datetime, timezone

WARMUP_RUNS = 1
MEASURED_RUNS = 3
K8S_REPO = "/tmp/benchmark-repos/kubernetes"
UAST_BIN = "/workspace/build/bin/uast"
CODEFANG_BIN = "/workspace/build/bin/codefang"
OUTPUT_DIR = "/workspace/docs/benchmarks"
RESULTS_FILE = os.path.join(OUTPUT_DIR, "kubernetes_benchmark_v2_results.json")


@dataclass
class RunMetrics:
    wall_time_s: float = 0.0
    peak_rss_mb: float = 0.0
    cpu_percent: float = 0.0
    user_time_s: float = 0.0
    sys_time_s: float = 0.0


@dataclass
class BenchmarkResult:
    tool: str
    category: str
    description: str
    version: str
    runs: list = field(default_factory=list)
    avg_wall_time_s: float = 0.0
    avg_peak_rss_mb: float = 0.0
    avg_cpu_percent: float = 0.0
    min_wall_time_s: float = 0.0
    max_wall_time_s: float = 0.0
    median_wall_time_s: float = 0.0
    stddev_wall_time_s: float = 0.0
    command: str = ""
    success: bool = True
    error: str = ""


def measure_command(cmd, timeout_s=600, env=None):
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    time_cmd = ["/usr/bin/time", "-v", "bash", "-c", cmd]
    start = time.monotonic()
    try:
        result = subprocess.run(time_cmd, stdout=subprocess.DEVNULL,
                                stderr=subprocess.PIPE, text=True,
                                timeout=timeout_s, env=merged_env)
        elapsed = time.monotonic() - start
    except subprocess.TimeoutExpired:
        return RunMetrics(wall_time_s=timeout_s), False, f"Timeout after {timeout_s}s"

    stderr = result.stderr or ""
    peak_rss_kb = 0; user_time = 0.0; sys_time = 0.0; cpu_pct = 0.0; wall_clock = elapsed
    for line in stderr.split("\n"):
        line = line.strip()
        if "Maximum resident set size" in line:
            m = re.search(r"(\d+)", line); peak_rss_kb = int(m.group(1)) if m else 0
        elif "User time" in line:
            m = re.search(r"([\d.]+)", line); user_time = float(m.group(1)) if m else 0
        elif "System time" in line:
            m = re.search(r"([\d.]+)", line); sys_time = float(m.group(1)) if m else 0
        elif "Percent of CPU" in line:
            m = re.search(r"(\d+)", line); cpu_pct = float(m.group(1)) if m else 0
        elif "wall clock" in line:
            m = re.search(r"(\d+):(\d+\.?\d*)", line)
            if m: wall_clock = int(m.group(1)) * 60 + float(m.group(2))
        elif "Exit status" in line:
            m = re.search(r"(\d+)", line)
            if m and int(m.group(1)) != 0:
                return RunMetrics(wall_time_s=wall_clock), False, f"exit code {m.group(1)}"
    return RunMetrics(wall_time_s=round(wall_clock, 3), peak_rss_mb=round(peak_rss_kb/1024, 1),
                      cpu_percent=cpu_pct, user_time_s=round(user_time, 3),
                      sys_time_s=round(sys_time, 3)), True, ""


def run_benchmark(tool, category, description, cmd, version="", timeout_s=600, env=None):
    print(f"\n{'='*70}")
    print(f"  {tool} ({category}): {description}")
    print(f"{'='*70}")
    result = BenchmarkResult(tool=tool, category=category, description=description,
                             version=version, command=cmd)
    for i in range(WARMUP_RUNS):
        print(f"  [warmup {i+1}/{WARMUP_RUNS}] ...", end="", flush=True)
        metrics, success, error = measure_command(cmd, timeout_s=timeout_s, env=env)
        if not success:
            print(f" FAILED: {error}"); result.success = False; result.error = error; return result
        print(f" {metrics.wall_time_s:.2f}s")
    run_times, run_rss, run_cpu = [], [], []
    for i in range(MEASURED_RUNS):
        print(f"  [run {i+1}/{MEASURED_RUNS}] ...", end="", flush=True)
        metrics, success, error = measure_command(cmd, timeout_s=timeout_s, env=env)
        if not success:
            print(f" FAILED: {error}"); result.success = False; result.error = error; return result
        run_times.append(metrics.wall_time_s); run_rss.append(metrics.peak_rss_mb)
        run_cpu.append(metrics.cpu_percent); result.runs.append(asdict(metrics))
        print(f" {metrics.wall_time_s:.2f}s | {metrics.peak_rss_mb:.0f}MB | {metrics.cpu_percent:.0f}% CPU")
    if run_times:
        n = len(run_times); st = sorted(run_times)
        result.avg_wall_time_s = round(sum(run_times)/n, 3)
        result.avg_peak_rss_mb = round(sum(run_rss)/n, 1)
        result.avg_cpu_percent = round(sum(run_cpu)/n, 1)
        result.min_wall_time_s = round(st[0], 3); result.max_wall_time_s = round(st[-1], 3)
        result.median_wall_time_s = round(st[n//2], 3)
        mean = result.avg_wall_time_s
        result.stddev_wall_time_s = round((sum((t-mean)**2 for t in run_times)/n)**0.5, 3)
    print(f"  => avg: {result.avg_wall_time_s:.2f}s | RSS: {result.avg_peak_rss_mb:.0f}MB | CPU: {result.avg_cpu_percent:.0f}%")
    return result


def main():
    print("=" * 70)
    print("  OPTIMIZED BENCHMARK: uast/codefang v2")
    print("=" * 70)

    codefang_env = {
        "PKG_CONFIG_PATH": "/workspace/third_party/libgit2/install/lib64/pkgconfig:/workspace/third_party/libgit2/install/lib/pkgconfig",
        "CGO_ENABLED": "1",
    }
    go_bin = os.path.expanduser("~/go/bin")
    local_bin = os.path.expanduser("~/.local/bin")
    env_with_path = {"PATH": f"{local_bin}:{go_bin}:{os.environ.get('PATH', '')}"}

    results = []

    # AST Parse Batch: uast optimized (parallel, no serialization)
    results.append(run_benchmark(
        "uast (optimized)", "ast_parse_batch",
        "Parse all Go files — parallel workers, no serialization (-w 4 -f none)",
        f"cd {K8S_REPO} && {UAST_BIN} parse --all -f none -w 4",
        version="dev-optimized", env=codefang_env, timeout_s=120,
    ))

    # AST Parse Batch: uast original (xargs -P 4 with JSON)
    results.append(run_benchmark(
        "uast (original)", "ast_parse_batch",
        "Parse all Go files — xargs -P 4, JSON output (original method)",
        f'find {K8S_REPO} -name "*.go" -not -path "*/vendor/*" -print0 | xargs -0 -P 4 -n 500 {UAST_BIN} parse > /dev/null 2>&1',
        version="dev-optimized-build", env=codefang_env, timeout_s=300,
    ))

    # AST Parse Batch: ast-grep (reference)
    results.append(run_benchmark(
        "ast-grep", "ast_parse_batch",
        "Search all Go files for function pattern (16k+ files)",
        f'sg -p "func \\$NAME" --lang go {K8S_REPO} > /dev/null 2>&1; true',
        version="0.41.0", env=env_with_path, timeout_s=120,
    ))

    # AST Parse Single: uast optimized
    large_file = f"{K8S_REPO}/pkg/apis/core/validation/validation_test.go"
    results.append(run_benchmark(
        "uast (optimized)", "ast_parse_single",
        "Parse single 30k-line file — lazy loading, no serialization",
        f"{UAST_BIN} parse -f none {large_file}",
        version="dev-optimized", env=codefang_env,
    ))

    # AST Parse Single: uast with JSON
    results.append(run_benchmark(
        "uast (json)", "ast_parse_single",
        "Parse single 30k-line file — lazy loading, JSON output",
        f"{UAST_BIN} parse {large_file} > /dev/null 2>&1",
        version="dev-optimized", env=codefang_env,
    ))

    # AST Parse Single: ast-grep (reference)
    results.append(run_benchmark(
        "ast-grep", "ast_parse_single",
        "Parse + search single file (30478 lines)",
        f'sg -p "func \\$NAME" --lang go {large_file} > /dev/null 2>&1; true',
        version="0.41.0", env=env_with_path,
    ))

    # Static complexity: codefang
    results.append(run_benchmark(
        "codefang", "complexity",
        "Cyclomatic + cognitive complexity via UAST",
        f"{CODEFANG_BIN} run -a static/complexity --format compact {K8S_REPO} > /dev/null 2>&1",
        version="dev-optimized", env=codefang_env, timeout_s=120,
    ))

    # Static complexity: gocyclo
    results.append(run_benchmark(
        "gocyclo", "complexity",
        "Cyclomatic complexity, Go only",
        f"gocyclo {K8S_REPO} > /dev/null 2>&1; true",
        version="latest", env=env_with_path,
    ))

    # Code counting: scc
    results.append(run_benchmark(
        "scc", "code_counting",
        "Lines of code + complexity",
        f"scc --no-cocomo {K8S_REPO} > /dev/null 2>&1",
        version="3.6.0", env=env_with_path,
    ))

    # Code counting: tokei
    results.append(run_benchmark(
        "tokei", "code_counting",
        "Lines of code (Rust)",
        f"tokei {K8S_REPO} > /dev/null 2>&1",
        version="12.1.2",
    ))

    # Save
    os.makedirs(OUTPUT_DIR, exist_ok=True)
    data = {
        "metadata": {
            "title": "Kubernetes Benchmark V2 (Optimized)",
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "optimizations": [
                "Parser reuse across files",
                "Parallel worker pool (-w N)",
                "Lazy language loading (sync.Once)",
                "Format 'none' (skip serialization)",
                "Smart file collection (skip .git, vendor)",
            ],
        },
        "results": [asdict(r) for r in results],
    }
    with open(RESULTS_FILE, "w") as f:
        json.dump(data, f, indent=2)
    print(f"\nResults saved to {RESULTS_FILE}")

    print("\n" + "=" * 100)
    print(f"{'TOOL':<25} {'CATEGORY':<20} {'AVG TIME':>10} {'PEAK RSS':>10} {'CPU %':>8} {'STATUS':>8}")
    print("=" * 100)
    for r in results:
        status = "OK" if r.success else "FAIL"
        print(f"{r.tool:<25} {r.category:<20} {r.avg_wall_time_s:>9.2f}s {r.avg_peak_rss_mb:>9.0f}MB "
              f"{r.avg_cpu_percent:>7.0f}% {status:>8}")
    print("=" * 100)


if __name__ == "__main__":
    main()

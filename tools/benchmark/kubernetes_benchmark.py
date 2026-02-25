#!/usr/bin/env python3
"""
Kubernetes Codebase Performance Benchmark Suite

Compares codefang/uast against competitive tools across multiple dimensions:
- Code counting & metrics (scc, tokei, cloc, gocloc)
- Cyclomatic complexity analysis (lizard, gocyclo, codefang)
- AST parsing (uast, ast-grep)

Measures: wall-clock time, peak RSS (MB), CPU utilization (%).
Each benchmark runs WARMUP_RUNS + MEASURED_RUNS times for statistical validity.
"""

import json
import os
import platform
import re
import resource
import subprocess
import sys
import time
from dataclasses import dataclass, field, asdict
from datetime import date, datetime, timezone
from pathlib import Path
from typing import Optional

WARMUP_RUNS = 1
MEASURED_RUNS = 3
K8S_REPO = "/tmp/benchmark-repos/kubernetes"
UAST_BIN = "/workspace/build/bin/uast"
CODEFANG_BIN = "/workspace/build/bin/codefang"
_BENCH_DATE = date.today().isoformat()
OUTPUT_DIR = f"/workspace/docs/benchmarks/{_BENCH_DATE}"
RESULTS_FILE = "kubernetes_benchmark_results.json"

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


def get_system_info():
    """Collect system information for benchmark context."""
    cpu_info = "unknown"
    cpu_count = os.cpu_count() or 0
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("model name"):
                    cpu_info = line.split(":")[1].strip()
                    break
    except (IOError, IndexError):
        pass

    mem_total = "unknown"
    try:
        with open("/proc/meminfo") as f:
            for line in f:
                if line.startswith("MemTotal"):
                    kb = int(line.split()[1])
                    mem_total = f"{kb / 1024 / 1024:.1f} GB"
                    break
    except (IOError, ValueError):
        pass

    return {
        "cpu_model": cpu_info,
        "cpu_cores": cpu_count,
        "memory_total": mem_total,
        "os": f"{platform.system()} {platform.release()}",
        "arch": platform.machine(),
        "go_version": run_cmd_simple("go version"),
        "python_version": platform.python_version(),
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }


def run_cmd_simple(cmd):
    """Run a command and return stdout stripped."""
    try:
        r = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=10)
        return r.stdout.strip()
    except Exception:
        return "unknown"


def measure_command(cmd, timeout_s=600, env=None):
    """Run a command and measure wall time, peak RSS, CPU utilization.

    Uses /usr/bin/time -v to capture resource usage from stderr.
    The exit status line from /usr/bin/time tells us the actual process exit code.
    """
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)

    time_cmd = f"/usr/bin/time -v sh -c '{cmd}'"

    start = time.monotonic()
    try:
        result = subprocess.run(
            time_cmd,
            shell=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_s,
            env=merged_env,
        )
        elapsed = time.monotonic() - start
    except subprocess.TimeoutExpired:
        elapsed = timeout_s
        return RunMetrics(
            wall_time_s=elapsed,
            peak_rss_mb=0,
            cpu_percent=0,
            user_time_s=0,
            sys_time_s=0,
        ), False, f"Timeout after {timeout_s}s"

    stderr = result.stderr or ""

    peak_rss_kb = 0
    user_time = 0.0
    sys_time = 0.0
    cpu_pct = 0.0
    wall_clock = elapsed
    exit_status = -1

    for line in stderr.split("\n"):
        line = line.strip()
        if "Maximum resident set size" in line:
            try:
                peak_rss_kb = int(re.search(r"(\d+)", line).group(1))
            except (AttributeError, ValueError):
                pass
        elif "User time" in line:
            try:
                user_time = float(re.search(r"([\d.]+)", line).group(1))
            except (AttributeError, ValueError):
                pass
        elif "System time" in line:
            try:
                sys_time = float(re.search(r"([\d.]+)", line).group(1))
            except (AttributeError, ValueError):
                pass
        elif "Percent of CPU" in line:
            try:
                cpu_pct = float(re.search(r"(\d+)", line).group(1))
            except (AttributeError, ValueError):
                pass
        elif "wall clock" in line:
            m = re.search(r"(\d+):(\d+\.?\d*)", line)
            if m:
                wall_clock = int(m.group(1)) * 60 + float(m.group(2))
        elif "Exit status" in line:
            try:
                exit_status = int(re.search(r"(\d+)", line).group(1))
            except (AttributeError, ValueError):
                pass

    success = exit_status == 0
    error = ""
    if not success:
        non_time_lines = [
            l.strip() for l in stderr.split("\n")
            if l.strip()
            and "Command being timed" not in l
            and "Maximum resident" not in l
            and "User time" not in l
            and "System time" not in l
            and "Percent of CPU" not in l
            and "wall clock" not in l
            and "Exit status" not in l
            and "page fault" not in l
            and "context switch" not in l
            and "Swap" not in l
            and "File system" not in l
            and "Socket" not in l
            and "Signal" not in l
            and "Page size" not in l
            and "Average" not in l
        ]
        error = "; ".join(non_time_lines[:3]) if non_time_lines else f"exit code {exit_status}"

    return RunMetrics(
        wall_time_s=round(wall_clock, 3),
        peak_rss_mb=round(peak_rss_kb / 1024, 1),
        cpu_percent=cpu_pct,
        user_time_s=round(user_time, 3),
        sys_time_s=round(sys_time, 3),
    ), success, error


def run_benchmark(tool, category, description, cmd, version="", timeout_s=600, env=None):
    """Run a benchmark with warmup and measured runs."""
    print(f"\n{'='*70}")
    print(f"  {tool} ({category})")
    print(f"  {description}")
    print(f"  cmd: {cmd}")
    print(f"{'='*70}")

    result = BenchmarkResult(
        tool=tool,
        category=category,
        description=description,
        version=version,
        command=cmd,
    )

    for i in range(WARMUP_RUNS):
        print(f"  [warmup {i+1}/{WARMUP_RUNS}] ...", end="", flush=True)
        metrics, success, error = measure_command(cmd, timeout_s=timeout_s, env=env)
        if not success:
            print(f" FAILED: {error}")
            result.success = False
            result.error = error
            return result
        print(f" {metrics.wall_time_s:.2f}s")

    run_times = []
    run_rss = []
    run_cpu = []

    for i in range(MEASURED_RUNS):
        print(f"  [run {i+1}/{MEASURED_RUNS}] ...", end="", flush=True)
        metrics, success, error = measure_command(cmd, timeout_s=timeout_s, env=env)
        if not success:
            print(f" FAILED: {error}")
            result.success = False
            result.error = error
            return result

        run_times.append(metrics.wall_time_s)
        run_rss.append(metrics.peak_rss_mb)
        run_cpu.append(metrics.cpu_percent)
        result.runs.append(asdict(metrics))

        print(f" {metrics.wall_time_s:.2f}s | {metrics.peak_rss_mb:.0f}MB | {metrics.cpu_percent:.0f}% CPU")

    if run_times:
        sorted_times = sorted(run_times)
        n = len(sorted_times)
        result.avg_wall_time_s = round(sum(run_times) / n, 3)
        result.avg_peak_rss_mb = round(sum(run_rss) / n, 1)
        result.avg_cpu_percent = round(sum(run_cpu) / n, 1)
        result.min_wall_time_s = round(sorted_times[0], 3)
        result.max_wall_time_s = round(sorted_times[-1], 3)
        result.median_wall_time_s = round(sorted_times[n // 2], 3)
        mean = result.avg_wall_time_s
        variance = sum((t - mean) ** 2 for t in run_times) / n
        result.stddev_wall_time_s = round(variance ** 0.5, 3)

    print(f"  => avg: {result.avg_wall_time_s:.2f}s | "
          f"peak RSS: {result.avg_peak_rss_mb:.0f}MB | "
          f"CPU: {result.avg_cpu_percent:.0f}%")

    return result


def get_tool_version(cmd):
    """Get a tool's version string."""
    return run_cmd_simple(cmd)


def drop_caches():
    """Attempt to drop filesystem caches for fair comparison."""
    try:
        subprocess.run("sync && echo 3 | sudo tee /proc/sys/vm/drop_caches",
                       shell=True, capture_output=True, timeout=5)
    except Exception:
        pass


def main():
    print("=" * 70)
    print("  KUBERNETES CODEBASE PERFORMANCE BENCHMARK")
    print(f"  Target: {K8S_REPO}")
    print(f"  Warmup runs: {WARMUP_RUNS} | Measured runs: {MEASURED_RUNS}")
    print("=" * 70)

    sys_info = get_system_info()
    print(f"\nSystem: {sys_info['cpu_model']} ({sys_info['cpu_cores']} cores)")
    print(f"Memory: {sys_info['memory_total']}")
    print(f"OS: {sys_info['os']}")

    go_path_bin = os.path.expanduser("~/go/bin")
    local_bin = os.path.expanduser("~/.local/bin")
    env_with_path = {"PATH": f"{local_bin}:{go_path_bin}:{os.environ.get('PATH', '')}"}

    versions = {
        "scc": get_tool_version(f"PATH={env_with_path['PATH']} scc --version"),
        "tokei": get_tool_version("tokei --version"),
        "cloc": get_tool_version("cloc --version"),
        "gocloc": "latest",
        "lizard": get_tool_version(f"PATH={env_with_path['PATH']} lizard --version"),
        "gocyclo": "latest",
        "ast-grep": get_tool_version(f"PATH={env_with_path['PATH']} sg --version"),
        "uast": get_tool_version(f"{UAST_BIN} version 2>/dev/null || echo dev"),
        "codefang": get_tool_version(f"{CODEFANG_BIN} version 2>/dev/null || echo dev"),
    }

    results = []

    # =========================================================================
    # CATEGORY 1: Code Counting & Metrics
    # =========================================================================
    print("\n\n" + "#" * 70)
    print("# CATEGORY 1: CODE COUNTING & METRICS")
    print("#" * 70)

    results.append(run_benchmark(
        tool="scc",
        category="code_counting",
        description="Lines of code, comments, blanks, complexity (Go, 250+ langs)",
        cmd=f"scc --no-cocomo {K8S_REPO} > /dev/null 2>&1",
        version=versions["scc"],
        env=env_with_path,
    ))

    results.append(run_benchmark(
        tool="tokei",
        category="code_counting",
        description="Lines of code, comments, blanks (Rust, fast)",
        cmd=f"tokei {K8S_REPO} > /dev/null 2>&1",
        version=versions["tokei"],
    ))

    results.append(run_benchmark(
        tool="cloc",
        category="code_counting",
        description="Lines of code, comments, blanks (Perl, classic)",
        cmd=f"cloc --quiet {K8S_REPO} > /dev/null 2>&1",
        version=versions["cloc"],
    ))

    results.append(run_benchmark(
        tool="gocloc",
        category="code_counting",
        description="Lines of code, comments, blanks (Go)",
        cmd=f"gocloc {K8S_REPO} > /dev/null 2>&1",
        version=versions["gocloc"],
        env=env_with_path,
    ))

    # =========================================================================
    # CATEGORY 2: Cyclomatic Complexity Analysis
    # =========================================================================
    print("\n\n" + "#" * 70)
    print("# CATEGORY 2: CYCLOMATIC COMPLEXITY ANALYSIS")
    print("#" * 70)

    results.append(run_benchmark(
        tool="lizard",
        category="complexity",
        description="Cyclomatic complexity, all languages (Python)",
        cmd=f"lizard -l go {K8S_REPO} > /dev/null 2>&1; true",
        version=versions["lizard"],
        env=env_with_path,
    ))

    results.append(run_benchmark(
        tool="gocyclo",
        category="complexity",
        description="Cyclomatic complexity, Go only (Go)",
        cmd=f"gocyclo {K8S_REPO} > /dev/null 2>&1; true",
        version=versions["gocyclo"],
        env=env_with_path,
    ))

    codefang_env = env_with_path.copy()
    codefang_env.update({
        "PKG_CONFIG_PATH": "/workspace/third_party/libgit2/install/lib64/pkgconfig:/workspace/third_party/libgit2/install/lib/pkgconfig",
        "CGO_ENABLED": "1",
    })

    results.append(run_benchmark(
        tool="codefang",
        category="complexity",
        description="Cyclomatic + cognitive complexity, multi-lang via UAST (Go+C)",
        cmd=f"{CODEFANG_BIN} run -a static/complexity --format compact {K8S_REPO} > /dev/null 2>&1",
        version=versions["codefang"],
        env=codefang_env,
    ))

    # =========================================================================
    # CATEGORY 3: AST Parsing (single large file)
    # =========================================================================
    print("\n\n" + "#" * 70)
    print("# CATEGORY 3: AST PARSING — SINGLE LARGE FILE")
    print("#" * 70)

    large_go_file = ""
    try:
        r = subprocess.run(
            f"find {K8S_REPO} -name '*.go' -not -path '*/vendor/*' -exec wc -l {{}} + | sort -rn | head -5",
            shell=True, capture_output=True, text=True, timeout=30,
        )
        lines = r.stdout.strip().split("\n")
        for line in lines:
            parts = line.strip().split()
            if len(parts) >= 2 and parts[1] != "total":
                large_go_file = parts[1]
                break
    except Exception:
        pass

    if large_go_file:
        line_count = run_cmd_simple(f"wc -l < {large_go_file}")
        print(f"\nLargest Go file: {large_go_file} ({line_count} lines)")

        results.append(run_benchmark(
            tool="uast",
            category="ast_parse_single",
            description=f"Parse single file to UAST, parse-only ({line_count} lines)",
            cmd=f"{UAST_BIN} parse -f none {large_go_file}",
            version=versions["uast"],
            env=codefang_env,
        ))

        results.append(run_benchmark(
            tool="ast-grep",
            category="ast_parse_single",
            description=f"Parse + search single file ({line_count} lines)",
            cmd=f'sg -p "func \\$NAME" --lang go {large_go_file} > /dev/null 2>&1; true',
            version=versions["ast-grep"],
            env=env_with_path,
        ))

    # =========================================================================
    # CATEGORY 4: AST Parsing (batch — all Go files)
    # =========================================================================
    print("\n\n" + "#" * 70)
    print("# CATEGORY 4: AST PARSING — BATCH (ALL GO FILES)")
    print("#" * 70)

    results.append(run_benchmark(
        tool="uast",
        category="ast_parse_batch",
        description="Parse all Go files, parallel parse-only (-w 4 -f none)",
        cmd=f'find {K8S_REPO} -name "*.go" -not -path "*/vendor/*" -print0 | xargs -0 {UAST_BIN} parse -f none',
        version=versions["uast"],
        env=codefang_env,
        timeout_s=300,
    ))

    results.append(run_benchmark(
        tool="ast-grep",
        category="ast_parse_batch",
        description="Search all Go files for function pattern (16k+ files)",
        cmd=f'sg -p "func \\$NAME" --lang go {K8S_REPO} > /dev/null 2>&1; true',
        version=versions["ast-grep"],
        env=env_with_path,
        timeout_s=300,
    ))

    # =========================================================================
    # CATEGORY 5: SCC complexity mode vs codefang
    # =========================================================================
    print("\n\n" + "#" * 70)
    print("# CATEGORY 5: COMPLEXITY WITH DETAIL (scc vs codefang)")
    print("#" * 70)

    results.append(run_benchmark(
        tool="scc",
        category="complexity_detail",
        description="SCC with complexity calculation enabled",
        cmd=f"scc --by-file --no-cocomo {K8S_REPO} > /dev/null 2>&1",
        version=versions["scc"],
        env=env_with_path,
    ))

    results.append(run_benchmark(
        tool="codefang",
        category="complexity_detail",
        description="Codefang full static complexity analysis",
        cmd=f"{CODEFANG_BIN} run -a static/complexity --format json {K8S_REPO} > /dev/null 2>&1",
        version=versions["codefang"],
        env=codefang_env,
    ))

    # =========================================================================
    # Aggregate and save results
    # =========================================================================
    os.makedirs(OUTPUT_DIR, exist_ok=True)

    benchmark_data = {
        "metadata": {
            "title": "Kubernetes Codebase Performance Benchmark",
            "target_repo": "kubernetes/kubernetes",
            "target_stats": {
                "total_files": int(run_cmd_simple(f"find {K8S_REPO} -type f | wc -l")),
                "go_files": int(run_cmd_simple(f"find {K8S_REPO} -name '*.go' | wc -l")),
                "go_lines": int(run_cmd_simple(f"find {K8S_REPO} -name '*.go' -exec cat {{}} + | wc -l")),
            },
            "system": sys_info,
            "config": {
                "warmup_runs": WARMUP_RUNS,
                "measured_runs": MEASURED_RUNS,
            },
        },
        "results": [asdict(r) for r in results],
    }

    results_path = os.path.join(OUTPUT_DIR, RESULTS_FILE)
    with open(results_path, "w") as f:
        json.dump(benchmark_data, f, indent=2)
    print(f"\nResults saved to {results_path}")

    # Print summary table
    print("\n\n" + "=" * 90)
    print(f"{'TOOL':<15} {'CATEGORY':<22} {'AVG TIME':>10} {'PEAK RSS':>10} {'CPU %':>8} {'STATUS':>8}")
    print("=" * 90)
    for r in results:
        status = "OK" if r.success else "FAIL"
        print(f"{r.tool:<15} {r.category:<22} {r.avg_wall_time_s:>9.2f}s {r.avg_peak_rss_mb:>9.0f}MB {r.avg_cpu_percent:>7.0f}% {status:>8}")
    print("=" * 90)

    return results


if __name__ == "__main__":
    main()

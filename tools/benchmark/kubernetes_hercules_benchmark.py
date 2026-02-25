#!/usr/bin/env python3
"""
Hercules vs Codefang: Git History Analyzer Benchmark on Kubernetes

Compares hercules (src-d) v10.7.2 against codefang on three history analyzers:
  - burndown (code survival over time)
  - couples (file coupling)
  - devs (developer activity)

Benchmarked at two commit scales: 500 and 1000 first-parent commits.
Measures: wall-clock time, peak RSS (MB), CPU utilization (%).
"""

import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass, field, asdict
from datetime import date, datetime, timezone

WARMUP_RUNS = 1
MEASURED_RUNS = 3
K8S_HISTORY_REPO = "/tmp/benchmark-repos/kubernetes-history"
HERCULES_BIN = "/tmp/hercules"
CODEFANG_BIN = "/workspace/build/bin/codefang"
_BENCH_DATE = date.today().isoformat()
OUTPUT_DIR = f"/workspace/docs/benchmarks/{_BENCH_DATE}"
RESULTS_FILE = os.path.join(OUTPUT_DIR, "kubernetes_hercules_benchmark_results.json")
COMMIT_SCALES = [500, 1000]


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
    analyzer: str
    commits: int
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
    speedup: float = 0.0


def measure_command(cmd, timeout_s=600, env=None):
    """Run a command and collect wall time, peak RSS, CPU % via GNU time."""
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)

    time_cmd = ["/usr/bin/time", "-v", "bash", "-c", cmd]

    start = time.monotonic()
    try:
        result = subprocess.run(
            time_cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_s,
            env=merged_env,
        )
        elapsed = time.monotonic() - start
    except subprocess.TimeoutExpired:
        return RunMetrics(wall_time_s=timeout_s), False, f"Timeout after {timeout_s}s"

    stderr = result.stderr or ""
    peak_rss_kb = 0
    user_time = 0.0
    sys_time = 0.0
    cpu_pct = 0.0
    wall_clock = elapsed

    for line in stderr.split("\n"):
        line = line.strip()
        if "Maximum resident set size" in line:
            m = re.search(r"(\d+)", line)
            if m:
                peak_rss_kb = int(m.group(1))
        elif "User time" in line:
            m = re.search(r"([\d.]+)", line)
            if m:
                user_time = float(m.group(1))
        elif "System time" in line:
            m = re.search(r"([\d.]+)", line)
            if m:
                sys_time = float(m.group(1))
        elif "Percent of CPU" in line:
            m = re.search(r"(\d+)", line)
            if m:
                cpu_pct = float(m.group(1))
        elif "wall clock" in line:
            m = re.search(r"(\d+):(\d+\.?\d*)", line)
            if m:
                wall_clock = int(m.group(1)) * 60 + float(m.group(2))
        elif "Exit status" in line:
            m = re.search(r"(\d+)", line)
            if m and int(m.group(1)) != 0:
                return RunMetrics(wall_time_s=wall_clock), False, f"exit code {m.group(1)}"

    return RunMetrics(
        wall_time_s=round(wall_clock, 3),
        peak_rss_mb=round(peak_rss_kb / 1024, 1),
        cpu_percent=cpu_pct,
        user_time_s=round(user_time, 3),
        sys_time_s=round(sys_time, 3),
    ), True, ""


def run_benchmark(tool, analyzer, commits, description, cmd, version="", timeout_s=600, env=None):
    """Run a benchmark with warmup and measured runs."""
    print(f"\n{'='*70}")
    print(f"  {tool} — {analyzer} ({commits} commits)")
    print(f"  {description}")
    print(f"{'='*70}")

    result = BenchmarkResult(
        tool=tool, analyzer=analyzer, commits=commits,
        description=description, version=version, command=cmd,
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

    run_times, run_rss, run_cpu = [], [], []

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
        n = len(run_times)
        sorted_times = sorted(run_times)
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


def generate_commit_file(repo, n_commits):
    """Generate a file with N first-parent commit hashes (oldest first)."""
    filepath = f"/tmp/k8s_{n_commits}_commits.txt"
    subprocess.run(
        f"cd {repo} && git log --first-parent --format='%H' -{n_commits} | tac > {filepath}",
        shell=True, capture_output=True,
    )
    return filepath


def main():
    print("=" * 70)
    print("  HERCULES vs CODEFANG: GIT HISTORY ANALYZER BENCHMARK")
    print(f"  Target: {K8S_HISTORY_REPO}")
    print(f"  Commit scales: {COMMIT_SCALES}")
    print(f"  Warmup: {WARMUP_RUNS} | Measured: {MEASURED_RUNS}")
    print("=" * 70)

    codefang_env = {
        "PKG_CONFIG_PATH": "/workspace/third_party/libgit2/install/lib64/pkgconfig:/workspace/third_party/libgit2/install/lib/pkgconfig",
        "CGO_ENABLED": "1",
    }

    hercules_version = subprocess.run(
        [HERCULES_BIN, "version"], capture_output=True, text=True,
    ).stdout.strip()
    codefang_version = subprocess.run(
        [CODEFANG_BIN, "version"], capture_output=True, text=True,
    ).stdout.strip()

    print(f"\nHercules: {hercules_version}")
    print(f"Codefang: {codefang_version}")

    analyzers = [
        ("burndown", "Code survival over time (line age tracking)"),
        ("couples", "File coupling analysis (co-change frequency)"),
        ("devs", "Developer activity (commits, additions, deletions per author)"),
    ]

    results = []

    for n_commits in COMMIT_SCALES:
        commit_file = generate_commit_file(K8S_HISTORY_REPO, n_commits)

        for analyzer_name, analyzer_desc in analyzers:
            print(f"\n\n{'#'*70}")
            print(f"# {analyzer_name.upper()} — {n_commits} commits")
            print(f"{'#'*70}")

            hercules_cmd = (
                f"{HERCULES_BIN} --{analyzer_name} --first-parent "
                f"--commits {commit_file} {K8S_HISTORY_REPO} > /dev/null 2>&1"
            )
            codefang_cmd = (
                f"{CODEFANG_BIN} run -a history/{analyzer_name} --format yaml "
                f"--first-parent --limit {n_commits} {K8S_HISTORY_REPO} > /dev/null 2>&1"
            )

            hr = run_benchmark(
                tool="hercules",
                analyzer=analyzer_name,
                commits=n_commits,
                description=f"Hercules {analyzer_desc}",
                cmd=hercules_cmd,
                version=hercules_version,
                timeout_s=600,
            )
            results.append(hr)

            cr = run_benchmark(
                tool="codefang",
                analyzer=analyzer_name,
                commits=n_commits,
                description=f"Codefang {analyzer_desc}",
                cmd=codefang_cmd,
                version=codefang_version,
                env=codefang_env,
                timeout_s=600,
            )
            results.append(cr)

            if hr.success and cr.success and cr.avg_wall_time_s > 0:
                speedup = round(hr.avg_wall_time_s / cr.avg_wall_time_s, 1)
                cr.speedup = speedup
                print(f"\n  >>> Codefang is {speedup}x faster than Hercules <<<")

    os.makedirs(OUTPUT_DIR, exist_ok=True)

    cpu_model = "unknown"
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("model name"):
                    cpu_model = line.split(":")[1].strip()
                    break
    except (IOError, IndexError):
        pass

    total_commits = subprocess.run(
        f"cd {K8S_HISTORY_REPO} && git log --first-parent --oneline | wc -l",
        shell=True, capture_output=True, text=True,
    ).stdout.strip()

    benchmark_data = {
        "metadata": {
            "title": "Hercules vs Codefang: Git History Analyzer Benchmark",
            "target_repo": "kubernetes/kubernetes",
            "target_stats": {
                "first_parent_commits": int(total_commits),
            },
            "system": {
                "cpu_model": cpu_model,
                "cpu_cores": os.cpu_count(),
                "os": f"Linux {os.uname().release}",
            },
            "tools": {
                "hercules": hercules_version,
                "codefang": codefang_version,
            },
            "config": {
                "warmup_runs": WARMUP_RUNS,
                "measured_runs": MEASURED_RUNS,
                "commit_scales": COMMIT_SCALES,
            },
            "timestamp": datetime.now(timezone.utc).isoformat(),
        },
        "results": [asdict(r) for r in results],
    }

    with open(RESULTS_FILE, "w") as f:
        json.dump(benchmark_data, f, indent=2)
    print(f"\nResults saved to {RESULTS_FILE}")

    # Summary table
    print("\n\n" + "=" * 100)
    print(f"{'TOOL':<12} {'ANALYZER':<12} {'COMMITS':>8} {'AVG TIME':>10} {'PEAK RSS':>10} "
          f"{'CPU %':>8} {'SPEEDUP':>10} {'STATUS':>8}")
    print("=" * 100)
    for r in results:
        status = "OK" if r.success else "FAIL"
        speedup_str = f"{r.speedup}x" if r.speedup > 0 else ""
        print(f"{r.tool:<12} {r.analyzer:<12} {r.commits:>8} {r.avg_wall_time_s:>9.2f}s "
              f"{r.avg_peak_rss_mb:>9.0f}MB {r.avg_cpu_percent:>7.0f}% {speedup_str:>10} {status:>8}")
    print("=" * 100)


if __name__ == "__main__":
    main()

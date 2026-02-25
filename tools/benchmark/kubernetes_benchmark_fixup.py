#!/usr/bin/env python3
"""
Fix-up runner for Kubernetes benchmarks.

Re-runs any benchmarks that failed in the initial run and merges
results into the main kubernetes_benchmark_results.json.

Usage: python3 tools/benchmark/kubernetes_benchmark_fixup.py
"""

import datetime
import json
import os
import re
import subprocess
import time
from dataclasses import dataclass, field, asdict

WARMUP_RUNS = 1
MEASURED_RUNS = 3
K8S_REPO = "/tmp/benchmark-repos/kubernetes"
UAST_BIN = "/workspace/build/bin/uast"
CODEFANG_BIN = "/workspace/build/bin/codefang"
_BENCH_DATE = datetime.date.today().isoformat()
OUTPUT_DIR = f"/workspace/docs/benchmarks/{_BENCH_DATE}"
RESULTS_FILE = os.path.join(OUTPUT_DIR, "kubernetes_benchmark_results.json")


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
    """Run a command via GNU time and collect resource usage."""
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
    exit_status = result.returncode

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
            if m:
                exit_status = int(m.group(1))

    return RunMetrics(
        wall_time_s=round(wall_clock, 3),
        peak_rss_mb=round(peak_rss_kb / 1024, 1),
        cpu_percent=cpu_pct,
        user_time_s=round(user_time, 3),
        sys_time_s=round(sys_time, 3),
    ), exit_status == 0, f"exit code {exit_status}" if exit_status != 0 else ""


def run_benchmark(tool, category, description, cmd, version="", timeout_s=600, env=None):
    """Run a benchmark with warmup and measured runs."""
    print(f"\n{'='*70}")
    print(f"  {tool} ({category}): {description}")
    print(f"{'='*70}")

    result = BenchmarkResult(
        tool=tool, category=category, description=description,
        version=version, command=cmd,
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


def main():
    """Re-run failed benchmarks and merge into existing results."""
    with open(RESULTS_FILE) as f:
        data = json.load(f)

    failed = [r for r in data["results"] if not r["success"]]
    if not failed:
        print("All benchmarks passed. Nothing to fix up.")
        return

    print(f"Found {len(failed)} failed benchmarks to re-run:")
    for r in failed:
        print(f"  - {r['tool']} ({r['category']}): {r['error']}")

    go_path_bin = os.path.expanduser("~/go/bin")
    local_bin = os.path.expanduser("~/.local/bin")
    env_with_path = {"PATH": f"{local_bin}:{go_path_bin}:{os.environ.get('PATH', '')}"}
    codefang_env = env_with_path.copy()
    codefang_env.update({
        "PKG_CONFIG_PATH": "/workspace/third_party/libgit2/install/lib64/pkgconfig:/workspace/third_party/libgit2/install/lib/pkgconfig",
        "CGO_ENABLED": "1",
    })

    for fr in failed:
        cmd = fr["command"]
        if "; true" not in cmd:
            cmd += "; true"

        nr = run_benchmark(
            tool=fr["tool"],
            category=fr["category"],
            description=fr["description"],
            cmd=cmd,
            version=fr["version"],
            env=codefang_env if fr["tool"] in ("codefang", "uast") else env_with_path,
        )

        if nr.success:
            for i, er in enumerate(data["results"]):
                if er["tool"] == nr.tool and er["category"] == nr.category:
                    data["results"][i] = asdict(nr)
                    break

    with open(RESULTS_FILE, "w") as f:
        json.dump(data, f, indent=2)
    print(f"\nUpdated results saved to {RESULTS_FILE}")


if __name__ == "__main__":
    main()

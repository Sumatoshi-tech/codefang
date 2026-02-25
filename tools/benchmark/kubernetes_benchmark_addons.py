#!/usr/bin/env python3
"""Add missing benchmarks to the main results file."""

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
_BENCH_DATE = datetime.date.today().isoformat()
OUTPUT_DIR = f"/workspace/docs/benchmarks/{_BENCH_DATE}"
RESULTS_FILE = os.path.join(OUTPUT_DIR, "kubernetes_benchmark_results.json")
LARGE_GO_FILE = f"{K8S_REPO}/pkg/apis/core/validation/validation_test.go"
LARGE_FILE_LINES = "30478"


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
    print(f"\n  {tool} ({category}): {description}")
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
    env_path = {"PATH": f"{os.path.expanduser('~/.local/bin')}:{os.path.expanduser('~/go/bin')}:{os.environ.get('PATH', '')}"}
    codefang_env = env_path.copy()
    codefang_env.update({
        "PKG_CONFIG_PATH": "/workspace/third_party/libgit2/install/lib64/pkgconfig:/workspace/third_party/libgit2/install/lib/pkgconfig",
        "CGO_ENABLED": "1",
    })

    new_results = []

    new_results.append(run_benchmark(
        "uast", "ast_parse_single",
        f"Parse single file to UAST, parse-only ({LARGE_FILE_LINES} lines)",
        f"{UAST_BIN} parse -f none {LARGE_GO_FILE}",
        version="dev", env=codefang_env,
    ))

    new_results.append(run_benchmark(
        "ast-grep", "ast_parse_single",
        f"Parse + search single file ({LARGE_FILE_LINES} lines)",
        f'sg -p "func \\$NAME" --lang go {LARGE_GO_FILE} > /dev/null 2>&1; true',
        version="0.41.0", env=env_path,
    ))

    with open(RESULTS_FILE) as f:
        data = json.load(f)

    for nr in new_results:
        if nr.success:
            found = False
            for i, er in enumerate(data["results"]):
                if er["tool"] == nr.tool and er["category"] == nr.category:
                    data["results"][i] = asdict(nr)
                    found = True
                    break
            if not found:
                data["results"].append(asdict(nr))

    with open(RESULTS_FILE, "w") as f:
        json.dump(data, f, indent=2)
    print(f"\nMerged into {RESULTS_FILE}")


if __name__ == "__main__":
    main()

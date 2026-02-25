#!/usr/bin/env python3
"""
Generate performance comparison plots from Kubernetes benchmark results.

Reads kubernetes_benchmark_results.json and produces:
  1. Wall-clock time comparison per category (bar chart)
  2. Peak RSS memory comparison per category (bar chart)
  3. CPU utilization comparison per category (bar chart)
  4. Overall dashboard (combined view)
  5. Radar/spider chart for multi-dimensional comparison
"""

import datetime
import json
import os
import sys
from pathlib import Path

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np

_BENCH_DATE = datetime.date.today().isoformat()
OUTPUT_DIR = f"/workspace/docs/benchmarks/{_BENCH_DATE}"
RESULTS_FILE = os.path.join(OUTPUT_DIR, "kubernetes_benchmark_results.json")

TOOL_COLORS = {
    "scc": "#2196F3",
    "tokei": "#FF9800",
    "cloc": "#9C27B0",
    "gocloc": "#4CAF50",
    "lizard": "#F44336",
    "gocyclo": "#00BCD4",
    "codefang": "#E91E63",
    "uast": "#E91E63",
    "ast-grep": "#607D8B",
}

CATEGORY_LABELS = {
    "code_counting": "Code Counting & Metrics",
    "complexity": "Cyclomatic Complexity",
    "ast_parse_single": "AST Parse (Single File)",
    "ast_parse_batch": "AST Parse (Batch)",
    "complexity_detail": "Detailed Complexity",
}


def load_results():
    """Load benchmark results from JSON."""
    with open(RESULTS_FILE) as f:
        return json.load(f)


def group_by_category(results):
    """Group results by category."""
    groups = {}
    for r in results:
        if not r["success"]:
            continue
        cat = r["category"]
        if cat not in groups:
            groups[cat] = []
        groups[cat].append(r)
    return groups


def plot_bar_chart(ax, category_results, metric_key, ylabel, title, fmt=".2f"):
    """Plot a horizontal bar chart for a specific metric."""
    tools = [r["tool"] for r in category_results]
    values = [r[metric_key] for r in category_results]
    colors = [TOOL_COLORS.get(t, "#999999") for t in tools]

    bars = ax.barh(tools, values, color=colors, edgecolor="white", height=0.6)

    for bar, val in zip(bars, values):
        label = f"{val:{fmt}}"
        if metric_key.endswith("_s"):
            label += "s"
        elif metric_key.endswith("_mb"):
            label += " MB"
        elif metric_key.endswith("_percent"):
            label += "%"
        ax.text(bar.get_width() + max(values) * 0.02, bar.get_y() + bar.get_height() / 2,
                label, va="center", fontsize=9, fontweight="bold")

    ax.set_xlabel(ylabel, fontsize=10)
    ax.set_title(title, fontsize=12, fontweight="bold", pad=10)
    ax.invert_yaxis()
    ax.set_xlim(0, max(values) * 1.25 if values else 1)
    ax.grid(axis="x", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)


def generate_category_charts(data):
    """Generate per-category comparison charts."""
    groups = group_by_category(data["results"])
    charts = []

    for cat, cat_results in groups.items():
        if len(cat_results) < 2:
            continue

        cat_label = CATEGORY_LABELS.get(cat, cat)

        fig, axes = plt.subplots(1, 3, figsize=(18, max(3, len(cat_results) * 0.8 + 2)))
        fig.suptitle(f"Kubernetes Benchmark: {cat_label}", fontsize=14, fontweight="bold", y=1.02)

        plot_bar_chart(axes[0], cat_results, "avg_wall_time_s", "Time (seconds)",
                       "Wall-Clock Time", fmt=".2f")
        plot_bar_chart(axes[1], cat_results, "avg_peak_rss_mb", "Memory (MB)",
                       "Peak RSS Memory", fmt=".0f")
        plot_bar_chart(axes[2], cat_results, "avg_cpu_percent", "CPU (%)",
                       "CPU Utilization", fmt=".0f")

        plt.tight_layout()
        fname = f"benchmark_{cat}.png"
        fpath = os.path.join(OUTPUT_DIR, fname)
        fig.savefig(fpath, dpi=150, bbox_inches="tight", facecolor="white")
        plt.close(fig)
        charts.append(fname)
        print(f"  Generated: {fname}")

    return charts


def generate_overall_time_chart(data):
    """Generate a single chart comparing wall-clock time across all tools/categories."""
    results = [r for r in data["results"] if r["success"]]
    if not results:
        return None

    fig, ax = plt.subplots(figsize=(14, max(4, len(results) * 0.55 + 2)))

    labels = [f"{r['tool']} ({CATEGORY_LABELS.get(r['category'], r['category'])})" for r in results]
    times = [r["avg_wall_time_s"] for r in results]
    colors = [TOOL_COLORS.get(r["tool"], "#999999") for r in results]

    sorted_data = sorted(zip(times, labels, colors), reverse=True)
    times, labels, colors = zip(*sorted_data)

    bars = ax.barh(labels, times, color=colors, edgecolor="white", height=0.6)

    for bar, val in zip(bars, times):
        ax.text(bar.get_width() + max(times) * 0.02, bar.get_y() + bar.get_height() / 2,
                f"{val:.2f}s", va="center", fontsize=9, fontweight="bold")

    ax.set_xlabel("Wall-Clock Time (seconds)", fontsize=11)
    ax.set_title("Kubernetes Codebase: Overall Performance Comparison",
                 fontsize=14, fontweight="bold", pad=15)
    ax.set_xlim(0, max(times) * 1.2)
    ax.grid(axis="x", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    meta = data["metadata"]
    info_text = (f"Target: kubernetes/kubernetes | "
                 f"{meta['target_stats']['go_files']:,} Go files | "
                 f"{meta['target_stats']['go_lines']:,} Go lines\n"
                 f"System: {meta['system']['cpu_model']} ({meta['system']['cpu_cores']} cores) | "
                 f"{meta['system']['memory_total']} RAM | "
                 f"{WARMUP_RUNS} warmup + {MEASURED_RUNS} measured runs")
    ax.text(0.5, -0.12, info_text, transform=ax.transAxes, ha="center",
            fontsize=8, color="gray", style="italic")

    plt.tight_layout()
    fname = "benchmark_overall_time.png"
    fpath = os.path.join(OUTPUT_DIR, fname)
    fig.savefig(fpath, dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_memory_overview(data):
    """Generate a chart comparing peak RSS across all tools."""
    results = [r for r in data["results"] if r["success"]]
    if not results:
        return None

    fig, ax = plt.subplots(figsize=(14, max(4, len(results) * 0.55 + 2)))

    labels = [f"{r['tool']} ({CATEGORY_LABELS.get(r['category'], r['category'])})" for r in results]
    rss = [r["avg_peak_rss_mb"] for r in results]
    colors = [TOOL_COLORS.get(r["tool"], "#999999") for r in results]

    sorted_data = sorted(zip(rss, labels, colors), reverse=True)
    rss, labels, colors = zip(*sorted_data)

    bars = ax.barh(labels, rss, color=colors, edgecolor="white", height=0.6)

    for bar, val in zip(bars, rss):
        ax.text(bar.get_width() + max(rss) * 0.02, bar.get_y() + bar.get_height() / 2,
                f"{val:.0f} MB", va="center", fontsize=9, fontweight="bold")

    ax.set_xlabel("Peak RSS Memory (MB)", fontsize=11)
    ax.set_title("Kubernetes Codebase: Memory Consumption Comparison",
                 fontsize=14, fontweight="bold", pad=15)
    ax.set_xlim(0, max(rss) * 1.2)
    ax.grid(axis="x", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    plt.tight_layout()
    fname = "benchmark_overall_memory.png"
    fpath = os.path.join(OUTPUT_DIR, fname)
    fig.savefig(fpath, dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_dashboard(data):
    """Generate a 2x2 dashboard with key comparisons."""
    groups = group_by_category(data["results"])
    fig, axes = plt.subplots(2, 2, figsize=(16, 12))
    fig.suptitle("Kubernetes Codebase: Performance Dashboard",
                 fontsize=16, fontweight="bold", y=1.01)

    plot_idx = 0
    metric_pairs = [
        ("code_counting", "avg_wall_time_s", "Time (s)", "Code Counting — Wall-Clock Time"),
        ("code_counting", "avg_peak_rss_mb", "Memory (MB)", "Code Counting — Peak Memory"),
        ("complexity", "avg_wall_time_s", "Time (s)", "Complexity Analysis — Wall-Clock Time"),
        ("complexity", "avg_peak_rss_mb", "Memory (MB)", "Complexity Analysis — Peak Memory"),
    ]

    for i, (cat, metric, ylabel, title) in enumerate(metric_pairs):
        ax = axes[i // 2][i % 2]
        if cat in groups:
            plot_bar_chart(ax, groups[cat], metric, ylabel, title)
        else:
            ax.text(0.5, 0.5, "No data", ha="center", va="center", transform=ax.transAxes)
            ax.set_title(title, fontsize=12, fontweight="bold")

    plt.tight_layout()
    fname = "benchmark_dashboard.png"
    fpath = os.path.join(OUTPUT_DIR, fname)
    fig.savefig(fpath, dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_time_vs_memory_scatter(data):
    """Generate a scatter plot: wall time vs peak RSS for each tool."""
    results = [r for r in data["results"] if r["success"]]
    if not results:
        return None

    fig, ax = plt.subplots(figsize=(12, 8))

    for r in results:
        color = TOOL_COLORS.get(r["tool"], "#999999")
        cat_label = CATEGORY_LABELS.get(r["category"], r["category"])
        ax.scatter(r["avg_wall_time_s"], r["avg_peak_rss_mb"],
                   c=color, s=150, edgecolors="black", linewidth=0.5, zorder=5)
        ax.annotate(f"{r['tool']}\n({cat_label})",
                    (r["avg_wall_time_s"], r["avg_peak_rss_mb"]),
                    textcoords="offset points", xytext=(10, 5),
                    fontsize=8, color=color, fontweight="bold")

    ax.set_xlabel("Wall-Clock Time (seconds)", fontsize=11)
    ax.set_ylabel("Peak RSS Memory (MB)", fontsize=11)
    ax.set_title("Kubernetes Benchmark: Time vs Memory Trade-off",
                 fontsize=14, fontweight="bold", pad=15)
    ax.grid(alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    ax.axhline(y=100, color="green", linestyle="--", alpha=0.3, label="100 MB baseline")
    ax.axvline(x=10, color="green", linestyle="--", alpha=0.3, label="10s baseline")

    plt.tight_layout()
    fname = "benchmark_time_vs_memory.png"
    fpath = os.path.join(OUTPUT_DIR, fname)
    fig.savefig(fpath, dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


WARMUP_RUNS = 1
MEASURED_RUNS = 3


def main():
    if not os.path.exists(RESULTS_FILE):
        print(f"Error: Results file not found: {RESULTS_FILE}")
        print("Run kubernetes_benchmark.py first.")
        sys.exit(1)

    data = load_results()
    WARMUP_RUNS = data["metadata"]["config"]["warmup_runs"]
    MEASURED_RUNS = data["metadata"]["config"]["measured_runs"]

    print("Generating benchmark plots...")
    print(f"Source: {RESULTS_FILE}")
    print()

    all_charts = []

    print("Per-category charts:")
    all_charts.extend(generate_category_charts(data))

    print("\nOverall charts:")
    c = generate_overall_time_chart(data)
    if c:
        all_charts.append(c)
    c = generate_memory_overview(data)
    if c:
        all_charts.append(c)
    c = generate_dashboard(data)
    if c:
        all_charts.append(c)
    c = generate_time_vs_memory_scatter(data)
    if c:
        all_charts.append(c)

    print(f"\nTotal charts generated: {len(all_charts)}")
    print(f"Output directory: {OUTPUT_DIR}")

    return all_charts


if __name__ == "__main__":
    main()

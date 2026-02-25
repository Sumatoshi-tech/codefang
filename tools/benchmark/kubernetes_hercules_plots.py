#!/usr/bin/env python3
"""
Generate plots for Hercules vs Codefang benchmark results.

Produces:
  1. Per-analyzer time comparison (grouped bar)
  2. Per-analyzer memory comparison (grouped bar)
  3. Speedup chart across analyzers and scales
  4. Time scaling chart (500 vs 1000 commits)
  5. Combined dashboard
"""

import datetime
import json
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np

_BENCH_DATE = datetime.date.today().isoformat()
OUTPUT_DIR = f"/workspace/docs/benchmarks/{_BENCH_DATE}"
RESULTS_FILE = os.path.join(OUTPUT_DIR, "kubernetes_hercules_benchmark_results.json")

TOOL_COLORS = {
    "hercules": "#607D8B",
    "codefang": "#E91E63",
}
ANALYZER_LABELS = {
    "burndown": "Burndown",
    "couples": "Couples",
    "devs": "Devs",
}


def load_results():
    with open(RESULTS_FILE) as f:
        return json.load(f)


def make_grouped_bar(ax, labels, herc_vals, codefang_vals, ylabel, title, fmt=".1f"):
    x = np.arange(len(labels))
    width = 0.35

    bars1 = ax.bar(x - width/2, herc_vals, width, label="Hercules v10.7.2",
                   color=TOOL_COLORS["hercules"], edgecolor="white")
    bars2 = ax.bar(x + width/2, codefang_vals, width, label="Codefang",
                   color=TOOL_COLORS["codefang"], edgecolor="white")

    for bar, val in zip(bars1, herc_vals):
        ax.text(bar.get_x() + bar.get_width()/2, bar.get_height() + max(herc_vals)*0.02,
                f"{val:{fmt}}", ha="center", va="bottom", fontsize=9, fontweight="bold",
                color=TOOL_COLORS["hercules"])
    for bar, val in zip(bars2, codefang_vals):
        ax.text(bar.get_x() + bar.get_width()/2, bar.get_height() + max(herc_vals)*0.02,
                f"{val:{fmt}}", ha="center", va="bottom", fontsize=9, fontweight="bold",
                color=TOOL_COLORS["codefang"])

    ax.set_xlabel("Analyzer", fontsize=10)
    ax.set_ylabel(ylabel, fontsize=10)
    ax.set_title(title, fontsize=12, fontweight="bold", pad=10)
    ax.set_xticks(x)
    ax.set_xticklabels(labels)
    ax.legend(fontsize=9)
    ax.grid(axis="y", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)


def generate_time_comparison(data):
    """Grouped bar: wall-clock time per analyzer, one chart per scale."""
    results = data["results"]
    scales = sorted(set(r["commits"] for r in results))

    fig, axes = plt.subplots(1, len(scales), figsize=(7 * len(scales), 6))
    if len(scales) == 1:
        axes = [axes]

    for i, scale in enumerate(scales):
        scale_results = [r for r in results if r["commits"] == scale and r["success"]]
        analyzers = sorted(set(r["analyzer"] for r in scale_results))
        labels = [ANALYZER_LABELS.get(a, a) for a in analyzers]

        herc_times = []
        codefang_times = []
        for a in analyzers:
            hr = next((r for r in scale_results if r["tool"] == "hercules" and r["analyzer"] == a), None)
            cr = next((r for r in scale_results if r["tool"] == "codefang" and r["analyzer"] == a), None)
            herc_times.append(hr["avg_wall_time_s"] if hr else 0)
            codefang_times.append(cr["avg_wall_time_s"] if cr else 0)

        make_grouped_bar(axes[i], labels, herc_times, codefang_times,
                         "Wall-Clock Time (s)",
                         f"Kubernetes — {scale} Commits")

    fig.suptitle("Hercules vs Codefang: Wall-Clock Time", fontsize=14, fontweight="bold", y=1.02)
    plt.tight_layout()
    fname = "hercules_time_comparison.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_memory_comparison(data):
    """Grouped bar: peak RSS per analyzer."""
    results = data["results"]
    scales = sorted(set(r["commits"] for r in results))

    fig, axes = plt.subplots(1, len(scales), figsize=(7 * len(scales), 6))
    if len(scales) == 1:
        axes = [axes]

    for i, scale in enumerate(scales):
        scale_results = [r for r in results if r["commits"] == scale and r["success"]]
        analyzers = sorted(set(r["analyzer"] for r in scale_results))
        labels = [ANALYZER_LABELS.get(a, a) for a in analyzers]

        herc_rss = []
        codefang_rss = []
        for a in analyzers:
            hr = next((r for r in scale_results if r["tool"] == "hercules" and r["analyzer"] == a), None)
            cr = next((r for r in scale_results if r["tool"] == "codefang" and r["analyzer"] == a), None)
            herc_rss.append(hr["avg_peak_rss_mb"] if hr else 0)
            codefang_rss.append(cr["avg_peak_rss_mb"] if cr else 0)

        make_grouped_bar(axes[i], labels, herc_rss, codefang_rss,
                         "Peak RSS (MB)",
                         f"Kubernetes — {scale} Commits",
                         fmt=".0f")

    fig.suptitle("Hercules vs Codefang: Peak Memory", fontsize=14, fontweight="bold", y=1.02)
    plt.tight_layout()
    fname = "hercules_memory_comparison.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_speedup_chart(data):
    """Horizontal bar: speedup factor per analyzer and scale."""
    results = data["results"]
    codefang_results = [r for r in results if r["tool"] == "codefang" and r["success"] and r["speedup"] > 0]

    if not codefang_results:
        return None

    labels = [f"{ANALYZER_LABELS.get(r['analyzer'], r['analyzer'])} ({r['commits']} commits)"
              for r in codefang_results]
    speedups = [r["speedup"] for r in codefang_results]

    fig, ax = plt.subplots(figsize=(12, max(3, len(labels) * 0.8 + 2)))

    colors = ["#E91E63" if s >= 25 else "#FF5722" if s >= 20 else "#FF9800" for s in speedups]
    bars = ax.barh(labels, speedups, color=colors, edgecolor="white", height=0.6)

    for bar, val in zip(bars, speedups):
        ax.text(bar.get_width() + 0.5, bar.get_y() + bar.get_height()/2,
                f"{val:.1f}x", va="center", fontsize=11, fontweight="bold")

    ax.set_xlabel("Speedup (Codefang / Hercules)", fontsize=11)
    ax.set_title("Codefang Speedup over Hercules on Kubernetes",
                 fontsize=14, fontweight="bold", pad=15)
    ax.axvline(x=1, color="gray", linestyle="--", alpha=0.5)
    ax.set_xlim(0, max(speedups) * 1.15)
    ax.invert_yaxis()
    ax.grid(axis="x", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    plt.tight_layout()
    fname = "hercules_speedup.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_scaling_chart(data):
    """Line chart showing how time scales with commit count for each tool/analyzer."""
    results = data["results"]
    scales = sorted(set(r["commits"] for r in results))
    analyzers = sorted(set(r["analyzer"] for r in results))

    fig, axes = plt.subplots(1, len(analyzers), figsize=(6 * len(analyzers), 5))
    if len(analyzers) == 1:
        axes = [axes]

    for i, analyzer in enumerate(analyzers):
        ax = axes[i]

        for tool in ["hercules", "codefang"]:
            tool_results = [r for r in results
                           if r["tool"] == tool and r["analyzer"] == analyzer and r["success"]]
            tool_results.sort(key=lambda r: r["commits"])

            commits = [r["commits"] for r in tool_results]
            times = [r["avg_wall_time_s"] for r in tool_results]

            marker = "s" if tool == "hercules" else "D"
            ax.plot(commits, times, marker=marker, linewidth=2,
                    color=TOOL_COLORS[tool], label=f"{tool.title()}", markersize=8)
            for x, y in zip(commits, times):
                ax.annotate(f"{y:.1f}s", (x, y), textcoords="offset points",
                            xytext=(10, 5), fontsize=9, fontweight="bold",
                            color=TOOL_COLORS[tool])

        ax.set_xlabel("Commits", fontsize=10)
        ax.set_ylabel("Wall-Clock Time (s)", fontsize=10)
        ax.set_title(f"{ANALYZER_LABELS.get(analyzer, analyzer)}", fontsize=12, fontweight="bold")
        ax.legend(fontsize=9)
        ax.grid(alpha=0.3)
        ax.spines["top"].set_visible(False)
        ax.spines["right"].set_visible(False)

    fig.suptitle("Time Scaling: Hercules vs Codefang", fontsize=14, fontweight="bold", y=1.02)
    plt.tight_layout()
    fname = "hercules_scaling.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_dashboard(data):
    """2x2 dashboard: time, memory, speedup, scaling."""
    results = data["results"]
    codefang_results = [r for r in results if r["tool"] == "codefang" and r["success"] and r["speedup"] > 0]
    scale_1000 = [r for r in results if r["commits"] == 1000 and r["success"]]

    fig, axes = plt.subplots(2, 2, figsize=(16, 12))
    fig.suptitle("Hercules vs Codefang: Kubernetes Performance Dashboard",
                 fontsize=16, fontweight="bold", y=1.01)

    # Top-left: time comparison (1000 commits)
    analyzers = sorted(set(r["analyzer"] for r in scale_1000))
    labels = [ANALYZER_LABELS.get(a, a) for a in analyzers]
    herc_t = [next((r["avg_wall_time_s"] for r in scale_1000 if r["tool"]=="hercules" and r["analyzer"]==a), 0) for a in analyzers]
    codefang_t = [next((r["avg_wall_time_s"] for r in scale_1000 if r["tool"]=="codefang" and r["analyzer"]==a), 0) for a in analyzers]
    make_grouped_bar(axes[0][0], labels, herc_t, codefang_t, "Time (s)", "Wall-Clock Time (1000 commits)")

    # Top-right: memory comparison (1000 commits)
    herc_m = [next((r["avg_peak_rss_mb"] for r in scale_1000 if r["tool"]=="hercules" and r["analyzer"]==a), 0) for a in analyzers]
    codefang_m = [next((r["avg_peak_rss_mb"] for r in scale_1000 if r["tool"]=="codefang" and r["analyzer"]==a), 0) for a in analyzers]
    make_grouped_bar(axes[0][1], labels, herc_m, codefang_m, "Memory (MB)", "Peak RSS (1000 commits)", fmt=".0f")

    # Bottom-left: speedup bars
    ax = axes[1][0]
    speedup_labels = [f"{ANALYZER_LABELS.get(r['analyzer'], r['analyzer'])} ({r['commits']})" for r in codefang_results]
    speedups = [r["speedup"] for r in codefang_results]
    colors = ["#E91E63" if s >= 25 else "#FF5722" if s >= 20 else "#FF9800" for s in speedups]
    bars = ax.barh(speedup_labels, speedups, color=colors, edgecolor="white", height=0.5)
    for bar, val in zip(bars, speedups):
        ax.text(bar.get_width() + 0.3, bar.get_y() + bar.get_height()/2,
                f"{val:.1f}x", va="center", fontsize=9, fontweight="bold")
    ax.set_xlabel("Speedup Factor")
    ax.set_title("Codefang Speedup over Hercules", fontsize=12, fontweight="bold")
    ax.invert_yaxis()
    ax.grid(axis="x", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    # Bottom-right: scaling lines
    ax = axes[1][1]
    scales = sorted(set(r["commits"] for r in results))
    for tool in ["hercules", "codefang"]:
        burndown = [r for r in results if r["tool"]==tool and r["analyzer"]=="burndown" and r["success"]]
        burndown.sort(key=lambda r: r["commits"])
        ax.plot([r["commits"] for r in burndown], [r["avg_wall_time_s"] for r in burndown],
                marker="o", linewidth=2, color=TOOL_COLORS[tool], label=f"{tool.title()} (burndown)")
    ax.set_xlabel("Commits")
    ax.set_ylabel("Time (s)")
    ax.set_title("Burndown Scaling", fontsize=12, fontweight="bold")
    ax.legend(fontsize=9)
    ax.grid(alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    plt.tight_layout()
    fname = "hercules_dashboard.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def main():
    if not os.path.exists(RESULTS_FILE):
        print(f"Error: {RESULTS_FILE} not found. Run kubernetes_hercules_benchmark.py first.")
        return

    data = load_results()
    print("Generating Hercules vs Codefang benchmark plots...")

    charts = []
    charts.append(generate_time_comparison(data))
    charts.append(generate_memory_comparison(data))
    c = generate_speedup_chart(data)
    if c:
        charts.append(c)
    charts.append(generate_scaling_chart(data))
    charts.append(generate_dashboard(data))

    print(f"\nTotal charts generated: {len(charts)}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
Generate before/after comparison plots for the UAST optimization.
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
V1_FILE = os.path.join(OUTPUT_DIR, "kubernetes_benchmark_results.json")
V2_FILE = os.path.join(OUTPUT_DIR, "kubernetes_benchmark_v2_results.json")

COLORS = {
    "before": "#607D8B",
    "after": "#E91E63",
    "competitor": "#2196F3",
}


def load_json(path):
    with open(path) as f:
        return json.load(f)


def generate_before_after_batch():
    """Before/after comparison for batch AST parsing."""
    v1 = load_json(V1_FILE)
    v2 = load_json(V2_FILE)

    v1_uast = next(r for r in v1["results"] if r["tool"] == "uast" and r["category"] == "ast_parse_batch")
    v1_astgrep = next(r for r in v1["results"] if r["tool"] == "ast-grep" and r["category"] == "ast_parse_batch")
    v2_uast_opt = next(r for r in v2["results"] if r["tool"] == "uast (optimized)" and r["category"] == "ast_parse_batch")
    v2_uast_orig = next(r for r in v2["results"] if r["tool"] == "uast (original)" and r["category"] == "ast_parse_batch")
    v2_astgrep = next(r for r in v2["results"] if r["tool"] == "ast-grep" and r["category"] == "ast_parse_batch")

    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))
    fig.suptitle("UAST Batch Parse Optimization: Before vs After", fontsize=14, fontweight="bold", y=1.02)

    labels = ["uast\n(before)", "uast\n(after, -w 4 -f none)", "ast-grep"]
    times = [v1_uast["avg_wall_time_s"], v2_uast_opt["avg_wall_time_s"], v2_astgrep["avg_wall_time_s"]]
    colors = [COLORS["before"], COLORS["after"], COLORS["competitor"]]
    bars = ax1.bar(labels, times, color=colors, edgecolor="white", width=0.6)
    for bar, val in zip(bars, times):
        ax1.text(bar.get_x() + bar.get_width()/2, bar.get_height() + 1,
                f"{val:.1f}s", ha="center", fontsize=11, fontweight="bold")
    ax1.set_ylabel("Wall-Clock Time (s)")
    ax1.set_title("Wall-Clock Time")
    ax1.grid(axis="y", alpha=0.3)
    ax1.spines["top"].set_visible(False)
    ax1.spines["right"].set_visible(False)

    rss = [v1_uast["avg_peak_rss_mb"], v2_uast_opt["avg_peak_rss_mb"], v2_astgrep["avg_peak_rss_mb"]]
    bars = ax2.bar(labels, rss, color=colors, edgecolor="white", width=0.6)
    for bar, val in zip(bars, rss):
        ax2.text(bar.get_x() + bar.get_width()/2, bar.get_height() + 30,
                f"{val:.0f} MB", ha="center", fontsize=11, fontweight="bold")
    ax2.set_ylabel("Peak RSS (MB)")
    ax2.set_title("Peak Memory")
    ax2.grid(axis="y", alpha=0.3)
    ax2.spines["top"].set_visible(False)
    ax2.spines["right"].set_visible(False)

    speedup = v1_uast["avg_wall_time_s"] / v2_uast_opt["avg_wall_time_s"]
    mem_reduction = v1_uast["avg_peak_rss_mb"] / v2_uast_opt["avg_peak_rss_mb"]
    fig.text(0.5, -0.04, f"Optimization: {speedup:.1f}x faster, {mem_reduction:.1f}x less memory | "
             f"16,620 Go files, Kubernetes codebase",
             ha="center", fontsize=10, style="italic", color="gray")

    plt.tight_layout()
    fname = "optimization_batch_parse.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_before_after_single():
    """Before/after comparison for single-file parsing."""
    v1 = load_json(V1_FILE)
    v2 = load_json(V2_FILE)

    v1_uast = next(r for r in v1["results"] if r["tool"] == "uast" and r["category"] == "ast_parse_single")
    v1_astgrep = next(r for r in v1["results"] if r["tool"] == "ast-grep" and r["category"] == "ast_parse_single")
    v2_uast_opt = next(r for r in v2["results"] if r["tool"] == "uast (optimized)" and r["category"] == "ast_parse_single")
    v2_uast_json = next(r for r in v2["results"] if r["tool"] == "uast (json)" and r["category"] == "ast_parse_single")
    v2_astgrep = next(r for r in v2["results"] if r["tool"] == "ast-grep" and r["category"] == "ast_parse_single")

    fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))
    fig.suptitle("UAST Single-File Parse Optimization: Before vs After", fontsize=14, fontweight="bold", y=1.02)

    labels = ["uast\n(before)", "uast\n(after, -f none)", "uast\n(after, JSON)", "ast-grep"]
    times = [v1_uast["avg_wall_time_s"], v2_uast_opt["avg_wall_time_s"],
             v2_uast_json["avg_wall_time_s"], v2_astgrep["avg_wall_time_s"]]
    colors = [COLORS["before"], COLORS["after"], "#FF9800", COLORS["competitor"]]
    bars = ax1.bar(labels, times, color=colors, edgecolor="white", width=0.55)
    for bar, val in zip(bars, times):
        ax1.text(bar.get_x() + bar.get_width()/2, bar.get_height() + 0.02,
                f"{val:.2f}s", ha="center", fontsize=10, fontweight="bold")
    ax1.set_ylabel("Wall-Clock Time (s)")
    ax1.set_title("Wall-Clock Time")
    ax1.grid(axis="y", alpha=0.3)
    ax1.spines["top"].set_visible(False)
    ax1.spines["right"].set_visible(False)

    rss = [v1_uast["avg_peak_rss_mb"], v2_uast_opt["avg_peak_rss_mb"],
           v2_uast_json["avg_peak_rss_mb"], v2_astgrep["avg_peak_rss_mb"]]
    bars = ax2.bar(labels, rss, color=colors, edgecolor="white", width=0.55)
    for bar, val in zip(bars, rss):
        ax2.text(bar.get_x() + bar.get_width()/2, bar.get_height() + 5,
                f"{val:.0f} MB", ha="center", fontsize=10, fontweight="bold")
    ax2.set_ylabel("Peak RSS (MB)")
    ax2.set_title("Peak Memory")
    ax2.grid(axis="y", alpha=0.3)
    ax2.spines["top"].set_visible(False)
    ax2.spines["right"].set_visible(False)

    speedup = v1_uast["avg_wall_time_s"] / v2_uast_opt["avg_wall_time_s"]
    mem_reduction = v1_uast["avg_peak_rss_mb"] / v2_uast_opt["avg_peak_rss_mb"]
    fig.text(0.5, -0.04, f"Lazy loading: {speedup:.1f}x faster, {mem_reduction:.1f}x less memory | "
             f"30,478-line Go file",
             ha="center", fontsize=10, style="italic", color="gray")

    plt.tight_layout()
    fname = "optimization_single_parse.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def generate_improvement_summary():
    """Summary chart showing all improvements."""
    improvements = [
        ("Single-file\nmemory", 389, 71, "MB"),
        ("Single-file\ntime", 0.99, 0.37, "s"),
        ("Batch parse\nmemory", 2779, 702, "MB"),
        ("Batch parse\ntime", 55.48, 25.63, "s"),
        ("Complexity\ntime", 39.32, 34.57, "s"),
    ]

    fig, ax = plt.subplots(figsize=(12, 6))

    labels = [i[0] for i in improvements]
    before = [i[1] for i in improvements]
    after = [i[2] for i in improvements]
    ratios = [b/a for b, a in zip(before, after)]

    x = np.arange(len(labels))
    width = 0.35

    bars1 = ax.bar(x - width/2, before, width, label="Before", color=COLORS["before"], edgecolor="white")
    bars2 = ax.bar(x + width/2, after, width, label="After", color=COLORS["after"], edgecolor="white")

    for bar, val, unit in zip(bars1, before, [i[3] for i in improvements]):
        ax.text(bar.get_x() + bar.get_width()/2, bar.get_height(),
                f"{val:.0f}{unit}" if val > 10 else f"{val:.2f}{unit}",
                ha="center", va="bottom", fontsize=8, color=COLORS["before"])
    for bar, val, ratio, unit in zip(bars2, after, ratios, [i[3] for i in improvements]):
        ax.text(bar.get_x() + bar.get_width()/2, bar.get_height(),
                f"{val:.0f}{unit}\n({ratio:.1f}x)" if val > 10 else f"{val:.2f}{unit}\n({ratio:.1f}x)",
                ha="center", va="bottom", fontsize=8, fontweight="bold", color=COLORS["after"])

    ax.set_xticks(x)
    ax.set_xticklabels(labels, fontsize=10)
    ax.set_title("UAST/Codefang Optimization Results on Kubernetes", fontsize=14, fontweight="bold")
    ax.legend(fontsize=11)
    ax.set_ylabel("Value (lower is better)")
    ax.grid(axis="y", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    plt.tight_layout()
    fname = "optimization_summary.png"
    fig.savefig(os.path.join(OUTPUT_DIR, fname), dpi=150, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"  Generated: {fname}")
    return fname


def main():
    print("Generating optimization comparison plots...")
    charts = []
    charts.append(generate_before_after_batch())
    charts.append(generate_before_after_single())
    charts.append(generate_improvement_summary())
    print(f"\nTotal: {len(charts)} charts")


if __name__ == "__main__":
    main()

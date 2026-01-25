#!/usr/bin/env python3
"""
UAST Performance Benchmark Comparison Tool

This script compares benchmark results between different test runs
and shows performance gains/losses in a prettified format.
"""

import sys
import json
import argparse
from pathlib import Path
from datetime import datetime
import pandas as pd

def load_results(results_dir):
    """Load benchmark results from a results directory."""
    results_file = Path("test/benchmarks") / results_dir / "results.json"
    if not results_file.exists():
        print(f"Error: Results file not found: {results_file}")
        return None
    
    with open(results_file, 'r') as f:
        return json.load(f)

def find_previous_runs():
    """Find all previous benchmark runs."""
    benchmarks_dir = Path("test/benchmarks")
    if not benchmarks_dir.exists():
        return []
    
    runs = []
    for run_dir in benchmarks_dir.iterdir():
        if run_dir.is_dir() and (run_dir / "results.json").exists():
            runs.append(run_dir.name)
    
    return sorted(runs, reverse=True)

def compare_benchmarks(baseline_data, current_data):
    """Compare benchmark results and calculate improvements."""
    baseline_benchmarks = {f"{b['benchmark_type']}/{b['test_case']}": b for b in baseline_data['benchmarks']}
    current_benchmarks = {f"{b['benchmark_type']}/{b['test_case']}": b for b in current_data['benchmarks']}
    
    comparisons = []
    
    for key in current_benchmarks:
        if key in baseline_benchmarks:
            baseline = baseline_benchmarks[key]
            current = current_benchmarks[key]
            
            # Calculate improvements
            time_improvement = ((baseline['time_ns'] - current['time_ns']) / baseline['time_ns']) * 100
            memory_improvement = ((baseline['memory_bytes'] - current['memory_bytes']) / baseline['memory_bytes']) * 100
            
            comparisons.append({
                'test': key,
                'baseline_time_ns': baseline['time_ns'],
                'current_time_ns': current['time_ns'],
                'time_improvement_pct': time_improvement,
                'baseline_memory_bytes': baseline['memory_bytes'],
                'current_memory_bytes': current['memory_bytes'],
                'memory_improvement_pct': memory_improvement,
                'baseline_time_ms': baseline['time_ms'],
                'current_time_ms': current['time_ms'],
                'baseline_memory_kb': baseline['memory_kb'],
                'current_memory_kb': current['memory_kb']
            })
    
    return comparisons

def format_improvement(value, is_percentage=True):
    """Format improvement value with color coding."""
    if is_percentage:
        if value > 0:
            return f"🟢 +{value:.1f}%"
        elif value < 0:
            return f"🔴 {value:.1f}%"
        else:
            return f"⚪ {value:.1f}%"
    else:
        return f"{value:.2f}"

def print_comparison_summary(baseline_run, current_run, comparisons):
    """Print a summary of the comparison."""
    print(f"\n{'='*80}")
    print(f"📊 PERFORMANCE COMPARISON")
    print(f"{'='*80}")
    print(f"Baseline: {baseline_run}")
    print(f"Current:  {current_run}")
    print(f"Tests compared: {len(comparisons)}")
    
    # Calculate overall improvements
    if comparisons:
        avg_time_improvement = sum(c['time_improvement_pct'] for c in comparisons) / len(comparisons)
        avg_memory_improvement = sum(c['memory_improvement_pct'] for c in comparisons) / len(comparisons)
        
        print(f"\n📈 OVERALL IMPROVEMENTS:")
        print(f"  Average Time Improvement: {format_improvement(avg_time_improvement)}")
        print(f"  Average Memory Improvement: {format_improvement(avg_memory_improvement)}")

def print_detailed_comparison(comparisons, show_all=False):
    """Print detailed comparison results."""
    print(f"\n📋 DETAILED RESULTS:")
    print(f"{'='*80}")
    
    # Group by benchmark type
    by_type = {}
    for comp in comparisons:
        benchmark_type = comp['test'].split('/')[0]
        if benchmark_type not in by_type:
            by_type[benchmark_type] = []
        by_type[benchmark_type].append(comp)
    
    for benchmark_type, type_comparisons in by_type.items():
        print(f"\n🔍 {benchmark_type.upper()}")
        print("-" * 60)
        
        # Sort by improvement (best first)
        type_comparisons.sort(key=lambda x: x['time_improvement_pct'], reverse=True)
        
        for comp in type_comparisons:
            test_name = comp['test'].split('/')[1]
            
            # Only show significant changes or if show_all is True
            significant_change = abs(comp['time_improvement_pct']) > 5 or abs(comp['memory_improvement_pct']) > 5
            
            if show_all or significant_change:
                print(f"  {test_name:<30} | "
                      f"Time: {format_improvement(comp['time_improvement_pct'])} | "
                      f"Memory: {format_improvement(comp['memory_improvement_pct'])}")
                
                if show_all or significant_change:
                    print(f"    {comp['baseline_time_ms']:.2f}ms → {comp['current_time_ms']:.2f}ms | "
                          f"{comp['baseline_memory_kb']:.1f}KB → {comp['current_memory_kb']:.1f}KB")

def print_top_improvements(comparisons, top_n=10):
    """Print top improvements and regressions."""
    if not comparisons:
        return
    
    print(f"\n🏆 TOP IMPROVEMENTS:")
    print("-" * 60)
    
    # Sort by time improvement
    sorted_by_time = sorted(comparisons, key=lambda x: x['time_improvement_pct'], reverse=True)
    for i, comp in enumerate(sorted_by_time[:top_n]):
        benchmark_type = comp['test'].split('/')[0]
        test_name = comp['test'].split('/')[1]
        full_test_name = f"{benchmark_type}/{test_name}"
        print(f"  {i+1:2d}. {full_test_name:<35} | Time: {format_improvement(comp['time_improvement_pct'])}")
    
    print(f"\n⚠️  TOP REGRESSIONS:")
    print("-" * 60)
    
    # Sort by worst time regression
    sorted_by_regression = sorted(comparisons, key=lambda x: x['time_improvement_pct'])
    for i, comp in enumerate(sorted_by_regression[:top_n]):
        benchmark_type = comp['test'].split('/')[0]
        test_name = comp['test'].split('/')[1]
        full_test_name = f"{benchmark_type}/{test_name}"
        print(f"  {i+1:2d}. {full_test_name:<35} | Time: {format_improvement(comp['time_improvement_pct'])}")

def export_comparison_csv(comparisons, output_file):
    """Export comparison results to CSV."""
    if not comparisons:
        return
    
    df = pd.DataFrame(comparisons)
    df.to_csv(output_file, index=False)
    print(f"\n💾 CSV exported to: {output_file}")

def main():
    parser = argparse.ArgumentParser(description="Compare UAST benchmark results")
    parser.add_argument("current_run", help="Current benchmark run directory name")
    parser.add_argument("--baseline", help="Baseline benchmark run directory name (default: previous run)")
    parser.add_argument("--all", action="store_true", help="Show all results, not just significant changes")
    parser.add_argument("--top", type=int, default=10, help="Number of top improvements/regressions to show")
    parser.add_argument("--export-csv", help="Export comparison results to CSV file")
    args = parser.parse_args()
    
    # Load current run
    current_data = load_results(args.current_run)
    if not current_data:
        sys.exit(1)
    
    # Determine baseline run
    baseline_run = args.baseline
    if not baseline_run:
        previous_runs = find_previous_runs()
        if len(previous_runs) < 2:
            print("Error: No previous runs found for comparison")
            sys.exit(1)
        baseline_run = previous_runs[1]  # Skip the current run, take the previous
    
    # Load baseline run
    baseline_data = load_results(baseline_run)
    if not baseline_data:
        sys.exit(1)
    
    # Compare results
    comparisons = compare_benchmarks(baseline_data, current_data)
    if not comparisons:
        print("Error: No matching benchmarks found for comparison")
        sys.exit(1)
    
    # Print results
    print_comparison_summary(baseline_run, args.current_run, comparisons)
    print_detailed_comparison(comparisons, show_all=args.all)
    print_top_improvements(comparisons, top_n=args.top)
    
    # Export CSV if requested
    if args.export_csv:
        export_comparison_csv(comparisons, args.export_csv)
    
    print(f"\n✅ Comparison completed!")

if __name__ == "__main__":
    main() 
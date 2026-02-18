#!/usr/bin/env python3
"""
UAST Performance Benchmark Multi-Comparison Tool

This script compares benchmark results across multiple test runs
and shows performance trends over time.
"""

import sys
import json
import argparse
from pathlib import Path
from datetime import datetime
import pandas as pd
from collections import defaultdict
import matplotlib.pyplot as plt

def load_results(results_dir):
    """Load benchmark results from a results directory."""
    results_file = Path("test/benchmarks") / results_dir / "results.json"
    if not results_file.exists():
        print(f"Error: Results file not found: {results_file}")
        return None
    
    with open(results_file, 'r') as f:
        return json.load(f)

def parse_runs_from_args(runs_str):
    """Parse run names from command line argument string."""
    if not runs_str:
        return []
    return [run.strip() for run in runs_str.split() if run.strip()]

def compare_multiple_runs(run_names):
    """Compare benchmark results across multiple runs."""
    if len(run_names) < 2:
        print("Error: Need at least 2 runs for comparison")
        return None
    
    # Load all runs
    runs_data = {}
    for run_name in run_names:
        data = load_results(run_name)
        if data:
            runs_data[run_name] = data
        else:
            print(f"Warning: Could not load run {run_name}")
    
    if len(runs_data) < 2:
        print("Error: Could not load at least 2 valid runs")
        return None
    
    # Find common benchmarks across all runs
    benchmark_keys = set()
    for run_data in runs_data.values():
        run_keys = {f"{b['benchmark_type']}/{b['test_case']}" for b in run_data['benchmarks']}
        if not benchmark_keys:
            benchmark_keys = run_keys
        else:
            benchmark_keys &= run_keys
    
    if not benchmark_keys:
        print("Error: No common benchmarks found across runs")
        return None
    
    # Build comparison data
    comparisons = {}
    for benchmark_key in benchmark_keys:
        benchmark_type, test_case = benchmark_key.split('/', 1)
        
        benchmark_data = []
        for run_name in run_names:
            if run_name in runs_data:
                run_data = runs_data[run_name]
                benchmark = next((b for b in run_data['benchmarks'] 
                                if f"{b['benchmark_type']}/{b['test_case']}" == benchmark_key), None)
                if benchmark:
                    benchmark_data.append({
                        'run': run_name,
                        'time_ns': benchmark['time_ns'],
                        'time_ms': benchmark['time_ms'],
                        'memory_bytes': benchmark['memory_bytes'],
                        'memory_kb': benchmark['memory_kb']
                    })
        
        if len(benchmark_data) >= 2:
            comparisons[benchmark_key] = benchmark_data
    
    return comparisons

def calculate_trends(comparisons):
    """Calculate performance trends across runs."""
    trends = {}
    
    for benchmark_key, benchmark_data in comparisons.items():
        if len(benchmark_data) < 2:
            continue
        
        # Sort by run order (assuming runs are in chronological order)
        sorted_data = sorted(benchmark_data, key=lambda x: x['run'])
        
        # Calculate trends
        first_run = sorted_data[0]
        last_run = sorted_data[-1]
        
        time_change = ((last_run['time_ns'] - first_run['time_ns']) / first_run['time_ns']) * 100
        memory_change = ((last_run['memory_bytes'] - first_run['memory_bytes']) / first_run['memory_bytes']) * 100
        
        # Calculate volatility (standard deviation of changes)
        time_values = [d['time_ns'] for d in sorted_data]
        memory_values = [d['memory_bytes'] for d in sorted_data]
        
        time_volatility = calculate_volatility(time_values)
        memory_volatility = calculate_volatility(memory_values)
        
        trends[benchmark_key] = {
            'first_run': first_run['run'],
            'last_run': last_run['run'],
            'runs_count': len(sorted_data),
            'time_change_pct': time_change,
            'memory_change_pct': memory_change,
            'time_volatility': time_volatility,
            'memory_volatility': memory_volatility,
            'data': sorted_data
        }
    
    return trends

def calculate_volatility(values):
    """Calculate coefficient of variation (volatility)."""
    if not values or len(values) < 2:
        return 0
    
    mean = sum(values) / len(values)
    if mean == 0:
        return 0
    
    variance = sum((x - mean) ** 2 for x in values) / len(values)
    std_dev = variance ** 0.5
    return (std_dev / mean) * 100

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

def print_multi_comparison_summary(run_names, trends):
    """Print a summary of the multi-run comparison."""
    print(f"\n{'='*80}")
    print(f"📊 MULTI-RUN PERFORMANCE COMPARISON")
    print(f"{'='*80}")
    print(f"Runs compared: {len(run_names)}")
    print(f"Runs: {' → '.join(run_names)}")
    print(f"Benchmarks compared: {len(trends)}")
    
    if trends:
        # Calculate overall trends
        time_changes = [t['time_change_pct'] for t in trends.values()]
        memory_changes = [t['memory_change_pct'] for t in trends.values()]
        
        avg_time_change = sum(time_changes) / len(time_changes)
        avg_memory_change = sum(memory_changes) / len(memory_changes)
        
        print(f"\n📈 OVERALL TRENDS:")
        print(f"  Average Time Change: {format_improvement(avg_time_change)}")
        print(f"  Average Memory Change: {format_improvement(avg_memory_change)}")

def print_trend_details(trends, show_all=False):
    """Print detailed trend results."""
    print(f"\n📋 TREND DETAILS:")
    print(f"{'='*80}")
    
    # Group by benchmark type
    by_type = defaultdict(list)
    for benchmark_key, trend in trends.items():
        benchmark_type = benchmark_key.split('/')[0]
        by_type[benchmark_type].append((benchmark_key, trend))
    
    for benchmark_type, type_trends in by_type.items():
        print(f"\n🔍 {benchmark_type.upper()}")
        print("-" * 60)
        
        # Sort by time change (best first)
        type_trends.sort(key=lambda x: x[1]['time_change_pct'], reverse=True)
        
        for benchmark_key, trend in type_trends:
            test_name = benchmark_key.split('/')[1]
            
            # Only show significant changes or if show_all is True
            significant_change = abs(trend['time_change_pct']) > 5 or abs(trend['memory_change_pct']) > 5
            
            if show_all or significant_change:
                print(f"  {test_name:<30} | "
                      f"Time: {format_improvement(trend['time_change_pct'])} | "
                      f"Memory: {format_improvement(trend['memory_change_pct'])}")
                
                if show_all or significant_change:
                    first_data = trend['data'][0]
                    last_data = trend['data'][-1]
                    print(f"    {first_data['time_ms']:.2f}ms → {last_data['time_ms']:.2f}ms | "
                          f"{first_data['memory_kb']:.1f}KB → {last_data['memory_kb']:.1f}KB")
                    print(f"    Volatility: Time {trend['time_volatility']:.1f}% | Memory {trend['memory_volatility']:.1f}%")

def print_top_trends(trends, top_n=10):
    """Print top improvements and regressions across all runs."""
    if not trends:
        return
    
    print(f"\n🏆 TOP IMPROVEMENTS:")
    print("-" * 60)
    
    # Sort by time improvement
    sorted_by_time = sorted(trends.items(), key=lambda x: x[1]['time_change_pct'], reverse=True)
    for i, (benchmark_key, trend) in enumerate(sorted_by_time[:top_n]):
        benchmark_type = benchmark_key.split('/')[0]
        test_name = benchmark_key.split('/')[1]
        full_test_name = f"{benchmark_type}/{test_name}"
        print(f"  {i+1:2d}. {full_test_name:<35} | Time: {format_improvement(trend['time_change_pct'])}")
    
    print(f"\n⚠️  TOP REGRESSIONS:")
    print("-" * 60)
    
    # Sort by worst time regression
    sorted_by_regression = sorted(trends.items(), key=lambda x: x[1]['time_change_pct'])
    for i, (benchmark_key, trend) in enumerate(sorted_by_regression[:top_n]):
        benchmark_type = benchmark_key.split('/')[0]
        test_name = benchmark_key.split('/')[1]
        full_test_name = f"{benchmark_type}/{test_name}"
        print(f"  {i+1:2d}. {full_test_name:<35} | Time: {format_improvement(trend['time_change_pct'])}")

def print_run_timeline(trends):
    """Print timeline of changes across runs."""
    print(f"\n📅 RUN TIMELINE:")
    print(f"{'='*80}")
    
    # Get all unique runs
    all_runs = set()
    for trend in trends.values():
        for data in trend['data']:
            all_runs.add(data['run'])
    
    all_runs = sorted(all_runs)
    
    print(f"Runs: {' → '.join(all_runs)}")
    print()
    
    # Show timeline for each benchmark
    for benchmark_key, trend in sorted(trends.items()):
        test_name = benchmark_key.split('/')[1]
        print(f"{test_name:<30} | ", end="")
        
        for run_name in all_runs:
            run_data = next((d for d in trend['data'] if d['run'] == run_name), None)
            if run_data:
                print(f"{run_data['time_ms']:6.2f}ms ", end="")
            else:
                print(f"{'N/A':>8} ", end="")
        print()

def export_trends_csv(trends, output_file):
    """Export trend results to CSV."""
    if not trends:
        return
    
    # Flatten trends data for CSV
    csv_data = []
    for benchmark_key, trend in trends.items():
        benchmark_type, test_case = benchmark_key.split('/', 1)
        
        csv_data.append({
            'benchmark_type': benchmark_type,
            'test_case': test_case,
            'first_run': trend['first_run'],
            'last_run': trend['last_run'],
            'runs_count': trend['runs_count'],
            'time_change_pct': trend['time_change_pct'],
            'memory_change_pct': trend['memory_change_pct'],
            'time_volatility': trend['time_volatility'],
            'memory_volatility': trend['memory_volatility']
        })
    
    df = pd.DataFrame(csv_data)
    df.to_csv(output_file, index=False)
    print(f"\n💾 CSV exported to: {output_file}")

def plot_trends(trends, run_names, output_file="trend_plot.png"):
    """Plot time trends for all benchmarks across runs."""
    plt.figure(figsize=(12, 7))
    all_runs = run_names
    for benchmark_key, trend in trends.items():
        times = []
        for run in all_runs:
            data = next((d for d in trend['data'] if d['run'] == run), None)
            times.append(data['time_ms'] if data else None)
        plt.plot(all_runs, times, marker='o', label=benchmark_key)
    plt.xlabel('Benchmark Run')
    plt.ylabel('Time (ms)')
    plt.title('Benchmark Time Trends Across Runs')
    plt.xticks(rotation=45, ha='right')
    plt.legend(fontsize='small', bbox_to_anchor=(1.05, 1), loc='upper left')
    plt.tight_layout()
    plt.savefig(output_file)
    print(f"\n📈 Trend plot saved to: {output_file}")

def main():
    parser = argparse.ArgumentParser(description="Compare UAST benchmark results across multiple runs")
    parser.add_argument("runs", help="Space-separated list of benchmark run directory names")
    parser.add_argument("--all", action="store_true", help="Show all results, not just significant changes")
    parser.add_argument("--top", type=int, default=10, help="Number of top improvements/regressions to show")
    parser.add_argument("--timeline", action="store_true", help="Show timeline of changes across runs")
    parser.add_argument("--export-csv", help="Export trend results to CSV file")
    parser.add_argument("--plot", nargs="?", const="trend_plot.png", help="Generate a trend plot (default: trend_plot.png)")
    args = parser.parse_args()
    
    # Parse run names
    run_names = parse_runs_from_args(args.runs)
    if len(run_names) < 2:
        print("Error: Need at least 2 run names")
        sys.exit(1)
    
    # Compare runs
    comparisons = compare_multiple_runs(run_names)
    if not comparisons:
        print("Error: No valid comparisons found")
        sys.exit(1)
    
    # Calculate trends
    trends = calculate_trends(comparisons)
    if not trends:
        print("Error: Could not calculate trends")
        sys.exit(1)
    
    # Print results
    print_multi_comparison_summary(run_names, trends)
    print_trend_details(trends, show_all=args.all)
    print_top_trends(trends, top_n=args.top)
    
    if args.timeline:
        print_run_timeline(trends)
    
    # Export CSV if requested
    if args.export_csv:
        export_trends_csv(trends, args.export_csv)
    
    if args.plot:
        plot_trends(trends, run_names, args.plot)
    
    print(f"\n✅ Multi-run comparison completed!")

if __name__ == "__main__":
    main() 
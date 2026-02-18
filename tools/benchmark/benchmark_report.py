#!/usr/bin/env python3
"""
UAST Performance Benchmark Report Generator

This script generates prettified reports for benchmark results.
"""

import sys
import json
import argparse
from pathlib import Path
from datetime import datetime

def load_results(results_dir):
    """Load benchmark results from a results directory."""
    results_file = Path("test/benchmarks") / results_dir / "results.json"
    if not results_file.exists():
        print(f"Error: Results file not found: {results_file}")
        return None
    
    with open(results_file, 'r') as f:
        return json.load(f)

def format_time(ns):
    """Format time in appropriate units."""
    if ns >= 1_000_000_000:  # 1 second
        return f"{ns / 1_000_000_000:.2f}s"
    elif ns >= 1_000_000:  # 1 millisecond
        return f"{ns / 1_000_000:.2f}ms"
    elif ns >= 1_000:  # 1 microsecond
        return f"{ns / 1_000:.2f}μs"
    else:
        return f"{ns}ns"

def format_memory(bytes_val):
    """Format memory in appropriate units."""
    if bytes_val >= 1024 * 1024:  # 1 MB
        return f"{bytes_val / (1024 * 1024):.1f}MB"
    elif bytes_val >= 1024:  # 1 KB
        return f"{bytes_val / 1024:.1f}KB"
    else:
        return f"{bytes_val}B"

def get_performance_category(time_ns, memory_bytes):
    """Categorize performance based on time and memory."""
    if time_ns < 1_000_000 and memory_bytes < 10_000:  # < 1ms and < 10KB
        return "🟢 Excellent"
    elif time_ns < 10_000_000 and memory_bytes < 100_000:  # < 10ms and < 100KB
        return "🟡 Good"
    elif time_ns < 100_000_000 and memory_bytes < 1_000_000:  # < 100ms and < 1MB
        return "🟠 Acceptable"
    else:
        return "🔴 Needs Optimization"

def print_header(git_info):
    """Print report header."""
    print(f"\n{'='*80}")
    print(f"📊 UAST PERFORMANCE BENCHMARK REPORT")
    print(f"{'='*80}")
    print(f"📅 Date/Time: {git_info['datetime']}")
    print(f"🔗 Git Commit: {git_info['hash']}")
    print(f"📝 Commit Message: {git_info['message']}")

def print_summary(data):
    """Print summary statistics."""
    benchmarks = data['benchmarks']
    summary = data['summary']
    
    print(f"\n📈 SUMMARY STATISTICS:")
    print(f"  Total Benchmarks: {summary['total_benchmarks']}")
    print(f"  Benchmark Types: {len(summary['benchmark_types'])}")
    print(f"  Test Cases: {len(summary['test_cases'])}")
    
    # Calculate averages
    avg_time = sum(b['time_ns'] for b in benchmarks) / len(benchmarks)
    avg_memory = sum(b['memory_bytes'] for b in benchmarks) / len(benchmarks)
    
    print(f"  Average Time: {format_time(avg_time)}")
    print(f"  Average Memory: {format_memory(avg_memory)}")

def print_benchmark_type_summary(data):
    """Print summary by benchmark type."""
    benchmarks = data['benchmarks']
    
    # Group by benchmark type
    by_type = {}
    for b in benchmarks:
        if b['benchmark_type'] not in by_type:
            by_type[b['benchmark_type']] = []
        by_type[b['benchmark_type']].append(b)
    
    print(f"\n🔍 BENCHMARK TYPE SUMMARY:")
    print(f"{'='*80}")
    
    for benchmark_type, type_benchmarks in by_type.items():
        avg_time = sum(b['time_ns'] for b in type_benchmarks) / len(type_benchmarks)
        avg_memory = sum(b['memory_bytes'] for b in type_benchmarks) / len(type_benchmarks)
        category = get_performance_category(avg_time, avg_memory)
        
        print(f"\n📋 {benchmark_type.upper()}")
        print(f"  Tests: {len(type_benchmarks)}")
        print(f"  Average Time: {format_time(avg_time)}")
        print(f"  Average Memory: {format_memory(avg_memory)}")
        print(f"  Performance: {category}")

def print_top_performers(data, top_n=10):
    """Print top performing benchmarks."""
    benchmarks = data['benchmarks']
    
    # Sort by time (fastest first)
    sorted_by_time = sorted(benchmarks, key=lambda x: x['time_ns'])
    
    print(f"\n🏆 FASTEST BENCHMARKS (Top {top_n}):")
    print(f"{'='*80}")
    
    for i, b in enumerate(sorted_by_time[:top_n]):
        test_name = f"{b['benchmark_type']}/{b['test_case']}"
        category = get_performance_category(b['time_ns'], b['memory_bytes'])
        print(f"  {i+1:2d}. {test_name:<40} | {format_time(b['time_ns']):<10} | {format_memory(b['memory_bytes']):<8} | {category}")

def print_slowest_benchmarks(data, top_n=10):
    """Print slowest benchmarks."""
    benchmarks = data['benchmarks']
    
    # Sort by time (slowest first)
    sorted_by_time = sorted(benchmarks, key=lambda x: x['time_ns'], reverse=True)
    
    print(f"\n🐌 SLOWEST BENCHMARKS (Top {top_n}):")
    print(f"{'='*80}")
    
    for i, b in enumerate(sorted_by_time[:top_n]):
        test_name = f"{b['benchmark_type']}/{b['test_case']}"
        category = get_performance_category(b['time_ns'], b['memory_bytes'])
        print(f"  {i+1:2d}. {test_name:<40} | {format_time(b['time_ns']):<10} | {format_memory(b['memory_bytes']):<8} | {category}")

def print_memory_usage(data, top_n=10):
    """Print highest memory usage benchmarks."""
    benchmarks = data['benchmarks']
    
    # Sort by memory usage (highest first)
    sorted_by_memory = sorted(benchmarks, key=lambda x: x['memory_bytes'], reverse=True)
    
    print(f"\n💾 HIGHEST MEMORY USAGE (Top {top_n}):")
    print(f"{'='*80}")
    
    for i, b in enumerate(sorted_by_memory[:top_n]):
        test_name = f"{b['benchmark_type']}/{b['test_case']}"
        category = get_performance_category(b['time_ns'], b['memory_bytes'])
        print(f"  {i+1:2d}. {test_name:<40} | {format_time(b['time_ns']):<10} | {format_memory(b['memory_bytes']):<8} | {category}")

def print_detailed_results(data, show_all=False):
    """Print detailed results for all benchmarks."""
    benchmarks = data['benchmarks']
    
    # Group by benchmark type
    by_type = {}
    for b in benchmarks:
        if b['benchmark_type'] not in by_type:
            by_type[b['benchmark_type']] = []
        by_type[b['benchmark_type']].append(b)
    
    print(f"\n📋 DETAILED RESULTS:")
    print(f"{'='*80}")
    
    for benchmark_type, type_benchmarks in by_type.items():
        print(f"\n🔍 {benchmark_type.upper()}")
        print("-" * 60)
        
        # Sort by time
        type_benchmarks.sort(key=lambda x: x['time_ns'])
        
        for b in type_benchmarks:
            test_name = b['test_case']
            category = get_performance_category(b['time_ns'], b['memory_bytes'])
            
            # Only show significant results or if show_all is True
            significant = b['time_ns'] > 1_000_000 or b['memory_bytes'] > 100_000  # > 1ms or > 100KB
            
            if show_all or significant:
                print(f"  {test_name:<30} | {format_time(b['time_ns']):<10} | {format_memory(b['memory_bytes']):<8} | {category}")

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

def main():
    parser = argparse.ArgumentParser(description="Generate UAST benchmark report")
    parser.add_argument("run_name", nargs="?", help="Benchmark run directory name (default: latest)")
    parser.add_argument("--all", action="store_true", help="Show all results, not just significant ones")
    parser.add_argument("--top", type=int, default=10, help="Number of top results to show")
    parser.add_argument("--list", action="store_true", help="List all available runs")
    args = parser.parse_args()
    
    if args.list:
        runs = find_previous_runs()
        if not runs:
            print("No benchmark runs found.")
            return
        
        print("Available benchmark runs:")
        for run in runs:
            print(f"  {run}")
        return
    
    # Determine which run to analyze
    run_name = args.run_name
    if not run_name:
        runs = find_previous_runs()
        if not runs:
            print("No benchmark runs found.")
            return
        run_name = runs[0]  # Use latest run
    
    # Load results
    data = load_results(run_name)
    if not data:
        sys.exit(1)
    
    # Print report
    print_header(data['git_info'])
    print_summary(data)
    print_benchmark_type_summary(data)
    print_top_performers(data, top_n=args.top)
    print_slowest_benchmarks(data, top_n=args.top)
    print_memory_usage(data, top_n=args.top)
    print_detailed_results(data, show_all=args.all)
    
    print(f"\n✅ Report generated for: {run_name}")

if __name__ == "__main__":
    main() 
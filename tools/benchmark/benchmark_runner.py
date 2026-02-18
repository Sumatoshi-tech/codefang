#!/usr/bin/env python3
"""
UAST Performance Benchmark Runner

This script runs benchmarks, organizes results by datetime and commit,
generates JSON results, and creates prettified reports.
"""

import sys
import os
import json
import subprocess
import datetime
import git
from pathlib import Path
import argparse

def get_git_info():
    """Get current git commit hash and message."""
    try:
        repo = git.Repo(search_parent_directories=True)
        commit = repo.head.commit
        return {
            'hash': commit.hexsha[:8],
            'message': commit.message.strip().split('\n')[0],
            'datetime': datetime.datetime.now().isoformat()
        }
    except Exception as e:
        return {
            'hash': 'unknown',
            'message': 'unknown',
            'datetime': datetime.datetime.now().isoformat()
        }

def run_benchmarks():
    """Run the benchmarks and capture output."""
    print("Running UAST benchmarks...")
    result = subprocess.run([
        'go', 'test', '-run=^$', '-bench=.', '-benchmem', '-benchtime=5s', './pkg/uast'
    ], capture_output=True, text=True, cwd=Path.cwd())
    
    if result.returncode != 0:
        print(f"Benchmark run failed: {result.stderr}")
        return None
    
    return result.stdout

def parse_benchmark_output(content):
    """Parse Go benchmark output and extract metrics."""
    lines = content.strip().split('\n')
    
    # Skip header lines
    data_lines = []
    for line in lines:
        if line.startswith('Benchmark') and 'ns/op' in line:
            data_lines.append(line)
    
    results = []
    for line in data_lines:
        # Parse benchmark line like:
        # BenchmarkParse/SmallGoFile-16      37734             31883 ns/op            6064 B/op
        import re
        match = re.match(r'Benchmark([^/]+)/([^-]+)-(\d+)\s+(\d+)\s+(\d+)\s+ns/op\s+(\d+)\s+B/op', line)
        if match:
            benchmark_type, test_case, cpu_count, iterations, time_ns, memory_bytes = match.groups()
            results.append({
                'benchmark_type': benchmark_type,
                'test_case': test_case,
                'cpu_count': int(cpu_count),
                'iterations': int(iterations),
                'time_ns': int(time_ns),
                'memory_bytes': int(memory_bytes),
                'time_ms': int(time_ns) / 1_000_000,  # Convert to milliseconds
                'memory_kb': int(memory_bytes) / 1024,  # Convert to KB
            })
    
    return results

def create_results_folder(git_info):
    """Create results folder with datetime and commit info."""
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    folder_name = f"{timestamp}_{git_info['hash']}_run"
    results_dir = Path("test/benchmarks") / folder_name
    results_dir.mkdir(parents=True, exist_ok=True)
    return results_dir

def save_results(results_dir, git_info, benchmark_data, raw_output):
    """Save all results to the folder."""
    # Save raw benchmark output
    with open(results_dir / "benchmark_output.txt", "w") as f:
        f.write(raw_output)
    
    # Save parsed JSON results
    results_data = {
        'git_info': git_info,
        'benchmarks': benchmark_data,
        'summary': {
            'total_benchmarks': len(benchmark_data),
            'benchmark_types': list(set(b['benchmark_type'] for b in benchmark_data)),
            'test_cases': list(set(b['test_case'] for b in benchmark_data))
        }
    }
    
    with open(results_dir / "results.json", "w") as f:
        json.dump(results_data, f, indent=2)
    
    # Save git info separately
    with open(results_dir / "git_info.json", "w") as f:
        json.dump(git_info, f, indent=2)

def generate_plots(results_dir, benchmark_data):
    """Generate performance plots."""
    print("Generating performance plots...")
    
    # Create plots directory
    plots_dir = results_dir / "plots"
    plots_dir.mkdir(exist_ok=True)
    
    # Run the existing plot script
    plot_script = Path("scripts/benchmark_plot.py")
    if plot_script.exists():
        subprocess.run([
            'python3', str(plot_script), 
            str(results_dir / "benchmark_output.txt")
        ], cwd=Path.cwd())
        
        # Move generated plots to results folder
        if Path("benchmark_plots").exists():
            for plot_file in Path("benchmark_plots").glob("*.png"):
                plot_file.rename(plots_dir / plot_file.name)
            for plot_file in Path("benchmark_plots").glob("*.txt"):
                plot_file.rename(plots_dir / plot_file.name)

def create_readme(results_dir, git_info):
    """Create a README file for the results."""
    readme_content = f"""# UAST Performance Benchmark Results

## Test Run Information
- **Date/Time**: {git_info['datetime']}
- **Git Commit**: {git_info['hash']}
- **Commit Message**: {git_info['message']}

## Files
- `benchmark_output.txt` - Raw Go benchmark output
- `results.json` - Parsed benchmark data in JSON format
- `git_info.json` - Git information for this run
- `plots/` - Performance visualization plots

## How to View Results
1. **Raw Data**: Check `benchmark_output.txt` for detailed Go benchmark output
2. **Structured Data**: Use `results.json` for programmatic analysis
3. **Visualizations**: View plots in the `plots/` directory
4. **Comparison**: Use the comparison script to compare with previous runs

## Performance Analysis
Run the comparison script to see performance gains/losses:
```bash
python3 scripts/benchmark_comparison.py {results_dir.name}
```
"""
    
    with open(results_dir / "README.md", "w") as f:
        f.write(readme_content)

def main():
    parser = argparse.ArgumentParser(description="Run UAST performance benchmarks")
    parser.add_argument("--no-plots", action="store_true", help="Skip plot generation")
    args = parser.parse_args()
    
    print("=== UAST Performance Benchmark Runner ===")
    
    # Get git information
    git_info = get_git_info()
    print(f"Git commit: {git_info['hash']} - {git_info['message']}")
    
    # Run benchmarks
    raw_output = run_benchmarks()
    if raw_output is None:
        sys.exit(1)
    
    # Parse results
    benchmark_data = parse_benchmark_output(raw_output)
    if not benchmark_data:
        print("No benchmark data found!")
        sys.exit(1)
    
    # Create results folder
    results_dir = create_results_folder(git_info)
    print(f"Results folder: {results_dir}")
    
    # Save results
    save_results(results_dir, git_info, benchmark_data, raw_output)
    
    # Generate plots (unless disabled)
    if not args.no_plots:
        generate_plots(results_dir, benchmark_data)
    
    # Create README
    create_readme(results_dir, git_info)
    
    print(f"\n✅ Benchmark run completed!")
    print(f"📁 Results saved to: {results_dir}")
    print(f"📊 Found {len(benchmark_data)} benchmark results")
    
    # Print summary
    print("\n📈 Quick Summary:")
    for benchmark_type in set(b['benchmark_type'] for b in benchmark_data):
        type_data = [b for b in benchmark_data if b['benchmark_type'] == benchmark_type]
        print(f"  {benchmark_type}: {len(type_data)} tests")

if __name__ == "__main__":
    main() 
#!/usr/bin/env python3
"""
UAST Performance Benchmark Plot Generator

This script generates performance plots from Go benchmark results.
Usage: python3 scripts/benchmark_plot.py <benchmark_output.txt>
"""

import sys
import re
import matplotlib.pyplot as plt
import pandas as pd
import numpy as np
from pathlib import Path

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
    
    return pd.DataFrame(results)

def create_performance_plots(df, output_dir='test/benchmark_plots'):
    """Create various performance plots from benchmark data."""
    Path(output_dir).mkdir(exist_ok=True)
    
    # Set style
    plt.style.use('default')
    plt.rcParams['figure.figsize'] = (12, 8)
    plt.rcParams['font.size'] = 10

    def add_value_labels(ax, orientation='v'):
        if orientation == 'v':
            for bar in ax.patches:
                ax.annotate(f'{bar.get_height():.1f}',
                            (bar.get_x() + bar.get_width() / 2, bar.get_height()),
                            ha='center', va='bottom', fontsize=8, rotation=0)
        else:
            for bar in ax.patches:
                ax.annotate(f'{bar.get_width():.1f}',
                            (bar.get_width(), bar.get_y() + bar.get_height() / 2),
                            ha='left', va='center', fontsize=8, rotation=0)

    # 1. Parsing Performance by File Size
    parsing_df = df[df['benchmark_type'] == 'Parse']
    if not parsing_df.empty:
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(16, 7))
        fig.suptitle('UAST Parsing Performance by File Size', fontsize=16, fontweight='bold')
        
        # Time plot
        bars1 = ax1.bar(parsing_df['test_case'], parsing_df['time_ms'], 
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        ax1.set_title('Parsing Time by File Size', fontsize=13)
        ax1.set_ylabel('Time per Parse (ms)')
        ax1.set_xlabel('File Size')
        ax1.tick_params(axis='x', rotation=45)
        add_value_labels(ax1, 'v')
        
        # Memory plot
        bars2 = ax2.bar(parsing_df['test_case'], parsing_df['memory_kb'],
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        ax2.set_title('Parsing Memory Usage by File Size', fontsize=13)
        ax2.set_ylabel('Memory per Parse (KB)')
        ax2.set_xlabel('File Size')
        ax2.tick_params(axis='x', rotation=45)
        add_value_labels(ax2, 'v')
        
        plt.figtext(0.5, 0.01, 'Each bar shows the average time and memory for parsing a file of the given size. "VeryLargeGoFile" is ~10x larger than "LargeGoFile".', ha='center', fontsize=10)
        plt.tight_layout(rect=[0, 0.03, 1, 0.95])
        plt.savefig(f'{output_dir}/parsing_performance.png', dpi=300, bbox_inches='tight')
        plt.close()

    # 2. DSL Query Performance
    dsl_df = df[df['benchmark_type'] == 'DSLQueries']
    if not dsl_df.empty:
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(16, 7))
        fig.suptitle('UAST DSL Query Performance', fontsize=16, fontweight='bold')
        
        # Time plot
        bars1 = ax1.barh(dsl_df['test_case'], dsl_df['time_ns'] / 1000,  # Convert to microseconds
                color='#2ca02c')
        ax1.set_title('Query Time by Query Type', fontsize=13)
        ax1.set_xlabel('Time per Query (μs)')
        add_value_labels(ax1, 'h')
        
        # Memory plot
        bars2 = ax2.barh(dsl_df['test_case'], dsl_df['memory_kb'],
                color='#ff7f0e')
        ax2.set_title('Query Memory Usage by Query Type', fontsize=13)
        ax2.set_xlabel('Memory per Query (KB)')
        add_value_labels(ax2, 'h')
        
        plt.figtext(0.5, 0.01, 'Each bar shows the average time and memory for a specific DSL query type.', ha='center', fontsize=10)
        plt.tight_layout(rect=[0, 0.03, 1, 0.95])
        plt.savefig(f'{output_dir}/dsl_query_performance.png', dpi=300, bbox_inches='tight')
        plt.close()

    # 3. Tree Traversal Performance
    traversal_df = df[df['benchmark_type'] == 'TreeTraversal']
    if not traversal_df.empty:
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(16, 7))
        fig.suptitle('UAST Tree Traversal Performance', fontsize=16, fontweight='bold')
        
        # Time plot
        bars1 = ax1.bar(traversal_df['test_case'], traversal_df['time_ns'] / 1000,
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728', '#9467bd', '#8c564b'])
        ax1.set_title('Traversal Time by Operation', fontsize=13)
        ax1.set_ylabel('Time per Traversal (μs)')
        ax1.tick_params(axis='x', rotation=45)
        add_value_labels(ax1, 'v')
        
        # Memory plot
        bars2 = ax2.bar(traversal_df['test_case'], traversal_df['memory_kb'],
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728', '#9467bd', '#8c564b'])
        ax2.set_title('Traversal Memory Usage by Operation', fontsize=13)
        ax2.set_ylabel('Memory per Traversal (KB)')
        ax2.tick_params(axis='x', rotation=45)
        add_value_labels(ax2, 'v')
        
        plt.figtext(0.5, 0.01, 'Each bar shows the average time and memory for a specific tree traversal operation.', ha='center', fontsize=10)
        plt.tight_layout(rect=[0, 0.03, 1, 0.95])
        plt.savefig(f'{output_dir}/tree_traversal_performance.png', dpi=300, bbox_inches='tight')
        plt.close()

    # 4. Change Detection Performance
    change_df = df[df['benchmark_type'] == 'ChangeDetection']
    if not change_df.empty:
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(16, 7))
        fig.suptitle('UAST Change Detection Performance', fontsize=16, fontweight='bold')
        
        # Time plot
        bars1 = ax1.bar(change_df['test_case'], change_df['time_ns'] / 1000,
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        ax1.set_title('Change Detection Time by Operation', fontsize=13)
        ax1.set_ylabel('Time per Detection (μs)')
        ax1.tick_params(axis='x', rotation=45)
        add_value_labels(ax1, 'v')
        
        # Memory plot
        bars2 = ax2.bar(change_df['test_case'], change_df['memory_kb'],
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        ax2.set_title('Change Detection Memory Usage by Operation', fontsize=13)
        ax2.set_ylabel('Memory per Detection (KB)')
        ax2.tick_params(axis='x', rotation=45)
        add_value_labels(ax2, 'v')
        
        plt.figtext(0.5, 0.01, 'Each bar shows the average time and memory for a specific change detection operation.', ha='center', fontsize=10)
        plt.tight_layout(rect=[0, 0.03, 1, 0.95])
        plt.savefig(f'{output_dir}/change_detection_performance.png', dpi=300, bbox_inches='tight')
        plt.close()

    # 5. Memory Usage Performance
    memory_df = df[df['benchmark_type'] == 'MemoryUsage']
    if not memory_df.empty:
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(16, 7))
        fig.suptitle('UAST Memory Usage Analysis', fontsize=16, fontweight='bold')
        
        # Time plot
        bars1 = ax1.bar(memory_df['test_case'], memory_df['time_ms'],
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        ax1.set_title('Memory Usage: Parse Time by File Size', fontsize=13)
        ax1.set_ylabel('Time per Parse (ms)')
        ax1.tick_params(axis='x', rotation=45)
        add_value_labels(ax1, 'v')
        
        # Memory plot
        bars2 = ax2.bar(memory_df['test_case'], memory_df['memory_kb'],
                color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        ax2.set_title('Memory Usage: Memory by File Size', fontsize=13)
        ax2.set_ylabel('Memory per Parse (KB)')
        ax2.tick_params(axis='x', rotation=45)
        add_value_labels(ax2, 'v')
        
        plt.figtext(0.5, 0.01, 'Each bar shows the average time and memory for parsing files of different sizes.', ha='center', fontsize=10)
        plt.tight_layout(rect=[0, 0.03, 1, 0.95])
        plt.savefig(f'{output_dir}/memory_usage_performance.png', dpi=300, bbox_inches='tight')
        plt.close()

    # 6. Summary Dashboard
    fig, axes = plt.subplots(2, 3, figsize=(20, 12))
    fig.suptitle('UAST Performance Benchmark Summary Dashboard', fontsize=18, fontweight='bold')
    
    # Parse performance
    if not parsing_df.empty:
        axes[0, 0].bar(parsing_df['test_case'], parsing_df['time_ms'], color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        axes[0, 0].set_title('Parsing Time (ms)')
        axes[0, 0].set_ylabel('ms')
        axes[0, 0].tick_params(axis='x', rotation=45)
        add_value_labels(axes[0, 0], 'v')
    # DSL performance
    if not dsl_df.empty:
        axes[0, 1].barh(dsl_df['test_case'], dsl_df['time_ns'] / 1000, color='#2ca02c')
        axes[0, 1].set_title('DSL Query Time (μs)')
        axes[0, 1].set_xlabel('μs')
        add_value_labels(axes[0, 1], 'h')
    # Traversal performance
    if not traversal_df.empty:
        axes[0, 2].bar(traversal_df['test_case'], traversal_df['time_ns'] / 1000, color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728', '#9467bd', '#8c564b'])
        axes[0, 2].set_title('Traversal Time (μs)')
        axes[0, 2].set_ylabel('μs')
        axes[0, 2].tick_params(axis='x', rotation=45)
        add_value_labels(axes[0, 2], 'v')
    # Parse memory
    if not parsing_df.empty:
        axes[1, 0].bar(parsing_df['test_case'], parsing_df['memory_kb'], color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        axes[1, 0].set_title('Parsing Memory (KB)')
        axes[1, 0].set_ylabel('KB')
        axes[1, 0].tick_params(axis='x', rotation=45)
        add_value_labels(axes[1, 0], 'v')
    # DSL memory
    if not dsl_df.empty:
        axes[1, 1].barh(dsl_df['test_case'], dsl_df['memory_kb'], color='#ff7f0e')
        axes[1, 1].set_title('DSL Query Memory (KB)')
        axes[1, 1].set_xlabel('KB')
        add_value_labels(axes[1, 1], 'h')
    # Change detection
    if not change_df.empty:
        axes[1, 2].bar(change_df['test_case'], change_df['time_ns'] / 1000, color=['#1f77b4', '#ff7f0e', '#2ca02c', '#d62728'])
        axes[1, 2].set_title('Change Detection Time (μs)')
        axes[1, 2].set_ylabel('μs')
        axes[1, 2].tick_params(axis='x', rotation=45)
        add_value_labels(axes[1, 2], 'v')
    plt.figtext(0.5, 0.01, 'Dashboard: Each section summarizes a key aspect of UAST performance. See individual plots for details.', ha='center', fontsize=11)
    plt.tight_layout(rect=[0, 0.03, 1, 0.95])
    plt.savefig(f'{output_dir}/performance_dashboard.png', dpi=300, bbox_inches='tight')
    plt.close()

    # Write a README.txt to the output directory
    with open(Path(output_dir) / 'README.txt', 'w') as f:
        f.write(
            """
UAST Performance Benchmark Plots
===============================

Each PNG file in this directory visualizes a different aspect of UAST performance:

- parsing_performance.png: Time and memory for parsing files of various sizes.
- dsl_query_performance.png: Time and memory for different DSL queries.
- tree_traversal_performance.png: Time and memory for tree traversal operations.
- change_detection_performance.png: Time and memory for change detection operations.
- memory_usage_performance.png: Parse time and memory for different file sizes.
- performance_dashboard.png: Summary dashboard of all key metrics.

Bar values are labeled for clarity. "VeryLargeGoFile" is ~10x larger than "LargeGoFile".

Interpretation:
- Lower time and memory bars indicate better performance.
- Compare across file sizes and operations to identify bottlenecks.
- Use dashboard for a high-level overview; see individual plots for details.
"""
        )
    print(f"Generated performance plots in {output_dir}/")
    print("Files created:")
    for plot_file in Path(output_dir).glob("*.png"):
        print(f"  - {plot_file}")

def main():
    if len(sys.argv) != 2:
        print("Usage: python3 scripts/benchmark_plot.py <benchmark_output.txt>")
        sys.exit(1)
    
    input_file = sys.argv[1]
    
    try:
        with open(input_file, 'r') as f:
            content = f.read()
        
        df = parse_benchmark_output(content)
        if df.empty:
            print("No benchmark data found in the input file.")
            sys.exit(1)
        
        create_performance_plots(df)
        
    except FileNotFoundError:
        print(f"Error: File '{input_file}' not found.")
        sys.exit(1)
    except Exception as e:
        print(f"Error processing benchmark data: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main() 
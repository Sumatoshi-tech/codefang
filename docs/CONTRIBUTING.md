# Contributing Guidelines

Codefang project is [Apache licensed](LICENSE.md) and accepts
contributions via GitHub pull requests.  This document outlines some of the
conventions on development workflow, commit message formatting, contact points,
and other resources to make it easier to get your contribution accepted.

## Certificate of Origin

By contributing to this project you agree to the [Developer Certificate of
Origin (DCO)](DCO). This document was created by the Linux Kernel community and is a
simple statement that you, as a contributor, have the legal right to make the
contribution.

In order to show your agreement with the DCO you should include at the end of commit message,
the following line: `Signed-off-by: John Doe <john.doe@example.com>`, using your real name.

This can be done easily using the [`-s`](https://github.com/git/git/blob/b2c150d3aa82f6583b9aadfecc5f8fa1c74aca09/Documentation/git-commit.txt#L154-L161) flag on the `git commit`.


## Support Channels

The official support channels, for both users and contributors, are:

- GitHub [issues](https://github.com/Sumatoshi-tech/codefang/issues)*
- Slack: #machine-learning room in the [source{d} Slack](https://join.slack.com/t/sourced-community/shared_invite/enQtMjc4Njk5MzEyNzM2LTFjNzY4NjEwZGEwMzRiNTM4MzRlMzQ4MmIzZjkwZmZlM2NjODUxZmJjNDI1OTcxNDAyMmZlNmFjODZlNTg0YWM)

*Before opening a new issue or submitting a new pull request, it's helpful to
search the project - it's likely that another user has already reported the
issue you're facing, or it's a known issue that we're already aware of.


## How to Contribute

Pull Requests (PRs) are the main and exclusive way to contribute to the official go-git project.
In order for a PR to be accepted it needs to pass a list of requirements:

- Code Coverage does not decrease.
- All the tests pass.
- Go code is idiomatic, formatted according to [gofmt](https://golang.org/cmd/gofmt/), and without any warnings from [go lint](https://github.com/golang/lint) nor [go vet](https://golang.org/cmd/vet/).
- Python code is formatted according to [![PEP8](https://img.shields.io/badge/code%20style-pep8-orange.svg)](https://www.python.org/dev/peps/pep-0008/).
- If the PR is a bug fix, it has to include a new unit test that fails before the patch is merged.
- If the PR is a new feature, it has to come with a suite of unit tests, that tests the new functionality.
- In any case, all the PRs have to pass the personal evaluation of at least one of the [maintainers](MAINTAINERS.md).


### Format of the commit message

The commit summary must start with a capital letter and with a verb in present tense. No dot in the end.

```
Add a feature
Remove unused code
Fix a bug
```

Every commit details should describe what was changed, under which context and, if applicable, the GitHub issue it relates to.

## Performance Benchmarking

The project includes a comprehensive benchmark management system to track performance improvements and regressions. This system is particularly important for UAST-related changes.

### Running Benchmarks

The benchmark system provides several commands for different scenarios:

```bash
# Run all benchmarks and generate plots
make benchplotv

# Run benchmarks only (no plots)
make benchv

# Run benchmarks with specific file sizes
make benchsmall    # Small files only
make benchmedium   # Medium files only  
make benchlarge    # Large files only
make benchxlarge   # Very large files only
```

### Benchmark Results Management

All benchmark results are automatically saved to the `benchmarks/` directory (which is gitignored). Each run creates a timestamped folder containing:

- Raw benchmark data in JSON format
- Performance plots (PNG format)
- Detailed reports with performance metrics

### Available Commands

```bash
# List all benchmark runs
make bench-list

# Generate a prettified report for the latest run
make report

# Compare two benchmark runs
make compare-runs CURRENT_RUN=<timestamp1> BASELINE_RUN=<timestamp2>

# Compare last two benchmark runs (simple)
make compare-last

# Run benchmarks with plots
make benchplotv

# Run benchmarks only (no plots)
make benchv

# Run full benchmark suite with organized results
make bench
```

### Performance Tracking

The benchmark system tracks several key metrics:

- **Parsing Performance**: Time to parse files of various sizes
- **Memory Usage**: Allocation patterns and memory efficiency
- **Query Performance**: DSL query execution times
- **Change Detection**: Performance of diff operations

### Performance Optimization Guidelines

When working on performance improvements:

1. **Baseline Measurement**: Always run benchmarks before making changes
2. **Incremental Testing**: Test changes with `make benchv` during development
3. **Documentation**: Update performance documentation when significant improvements are made
4. **Regression Prevention**: Ensure new code doesn't introduce performance regressions

### Benchmark System Components

- **Python Scripts** (in `scripts/`):
  - `benchmark_runner.py`: Orchestrates benchmark execution
  - `benchmark_report.py`: Generates human-readable reports
  - `benchmark_comparison.py`: Compares benchmark runs

- **Makefile Targets**: Provide convenient access to all benchmark operations
- **Results Storage**: Organized in `benchmarks/{timestamp}_commit_run/` folders

### Performance Roadmap

The project maintains a performance optimization roadmap in `PERFORMANCE_ROADMAP.md` that outlines planned improvements and tracks completed optimizations. Contributors should refer to this document when working on performance-related features.

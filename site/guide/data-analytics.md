# Data Analytics & DWH Integration

Codefang produces richly structured JSON output designed for loading into
columnar data warehouses (ClickHouse, Greenplum, BigQuery, Snowflake) and
building BI dashboards. This guide covers the optimal pipeline from
repository analysis to production dashboards.

---

## Quick Start

Analyze a repository and produce DWH-ready output:

```bash
# JSON for small-to-medium repos (< 5K files, < 10K commits)
codefang run --format json --per-file --memory-budget 4GB /path/to/repo > report.json

# NDJSON for large repos (streaming, one line per analyzer)
codefang run --format ndjson --per-file --memory-budget 8GB /path/to/repo > report.ndjson

# Limit history depth for faster iteration
codefang run --format json --per-file --limit 5000 /path/to/repo > report.json
```

---

## Output Format Selection

| Repo Size | Recommended Format | Reason |
|-----------|-------------------|--------|
| < 1K files | `json` | Small file, easy to inspect |
| 1K-10K files | `json` | Manageable (< 500MB typically) |
| 10K-50K files | `ndjson` | JSON gets multi-GB; NDJSON streams |
| 50K+ files | `ndjson` + `--limit` | Bound history for practical runtimes |

### JSON Format

```bash
codefang run --format json --per-file /repo > report.json
```

Produces a single JSON object with versioned envelope:

```json
{
  "version": "codefang.run.v1",
  "metadata": {
    "repo_name": "myproject",
    "analyzed_at": "2026-04-08T10:00:00Z",
    "codefang_version": "0.1.0"
  },
  "analyzers": [
    {
      "id": "static/complexity",
      "mode": "static",
      "schema": { ... },
      "report": { ... }
    }
  ]
}
```

### NDJSON Format

```bash
codefang run --format ndjson --per-file /repo > report.ndjson
```

One JSON line per analyzer. First line is metadata:

```
{"version":"codefang.run.v1","metadata":{"repo_name":"myproject",...}}
{"id":"static/complexity","mode":"static","report":{...}}
{"id":"history/sentiment","mode":"history","report":{...}}
```

Process with standard tools:

```bash
# Extract one analyzer
grep '"static/complexity"' report.ndjson | jq .report.aggregate

# Count analyzers
wc -l report.ndjson

# Stream into ClickHouse
cat report.ndjson | clickhouse-client --query "INSERT INTO codefang_raw FORMAT JSONEachRow"
```

---

## Memory Budget

**Always set `--memory-budget`** for repos with history analysis. Without it,
the streaming pipeline uses a conservative 2GB default that may OOM on large
repos.

| Machine RAM | Recommended Budget | Handles |
|-------------|-------------------|---------|
| 8 GB | `--memory-budget 2GB` | Repos up to ~10K commits |
| 16 GB | `--memory-budget 4GB` | Repos up to ~30K commits |
| 32 GB | `--memory-budget 8GB` | Repos up to ~60K commits |
| 64 GB | `--memory-budget 16GB` | Repos up to ~150K commits |

The budget controls the streaming chunk planner — larger budgets mean fewer,
bigger chunks and faster processing. The actual RSS will be ~2x the budget
due to Go runtime overhead and native memory.

```bash
# 64GB machine, kubernetes-sized repo (~56K commits)
codefang run --format ndjson --per-file --memory-budget 8GB ~/sources/kubernetes
```

!!! warning "Without `--memory-budget`"
    The default 2GB budget may cause the process to be killed by the OS OOM
    killer on large repos. Always set this flag explicitly.

---

## Commit Limiting

Use `--limit N` to analyze only the most recent N commits. This is useful for:

- **Fast iteration**: Test your ETL pipeline on a subset before running full history
- **Incremental analysis**: Analyze only recent changes for daily dashboards
- **Memory control**: Fewer commits = less memory, faster processing

```bash
# Last 1000 commits (fast, ~2 min)
codefang run --format json --per-file --limit 1000 /repo > recent.json

# Last 10000 commits (moderate, ~15 min)
codefang run --format json --per-file --limit 10000 --memory-budget 4GB /repo > report.json

# Full history (slow, may take hours for large repos)
codefang run --format json --per-file --memory-budget 8GB /repo > full.json
```

---

## Key Fields for Analytics

Every function-level record includes fields designed for DWH joins and
aggregation:

| Field | Present On | Type | Example | Use Case |
|-------|-----------|------|---------|----------|
| `source_file` | All function records | string | `"pkg/api/server.go"` | Join to file-level data |
| `language` | All function records | string | `"go"` | Group by language |
| `directory` | All function records | string | `"pkg/api"` | Group by package/module |
| `start_time` | All time-series ticks | RFC 3339 | `"2024-01-15T10:30:00Z"` | Time-axis labels |
| `end_time` | All time-series ticks | RFC 3339 | `"2024-01-16T08:45:00Z"` | Tick duration |
| `name` | Developer records | string | `"alice"` | Developer dimension |
| `email` | Developer records | string | `"alice@example.com"` | Developer identity |
| `dev_id` | Activity, contributors | int | `42` | Foreign key to developers |

---

## Schema Manifest

Every analyzer section includes a `schema` field describing its output:

```json
{
  "schema": {
    "function_complexity": {
      "type": "list",
      "grain": "function",
      "description": "Per-function cyclomatic and cognitive complexity"
    },
    "aggregate": {
      "type": "aggregate",
      "description": "Summary statistics"
    }
  }
}
```

**Field types**: `list`, `aggregate`, `time_series`, `risk`, `scalar`

**Grain values**: `function`, `file`, `tick`, `pair`, `developer`, `node`, `comment`, `import`

Use the schema to auto-generate ETL mappings:

```python
# Python: extract schema for table generation
import json
with open('report.json') as f:
    data = json.load(f)
for analyzer in data['analyzers']:
    schema = analyzer.get('schema', {})
    for field, meta in schema.items():
        if meta['type'] == 'list':
            print(f"CREATE TABLE {analyzer['id'].replace('/', '_')}_{field} ...")
```

---

## Star Schema Design

### Dimensions

```sql
-- dim_repository
CREATE TABLE dim_repository (
    repo_id     UInt64,
    repo_name   String,
    repo_path   String,
    analyzed_at DateTime,
    version     String
) ENGINE = MergeTree() ORDER BY repo_id;

-- dim_file (extract from source_file + directory + language)
CREATE TABLE dim_file (
    file_id     UInt64,
    repo_id     UInt64,
    source_file String,
    directory   String,
    language    LowCardinality(String)
) ENGINE = MergeTree() ORDER BY (repo_id, source_file);

-- dim_developer
CREATE TABLE dim_developer (
    dev_id  UInt32,
    repo_id UInt64,
    name    String,
    email   String
) ENGINE = MergeTree() ORDER BY (repo_id, dev_id);

-- dim_tick
CREATE TABLE dim_tick (
    tick_id    UInt32,
    repo_id    UInt64,
    tick       UInt32,
    start_time DateTime,
    end_time   DateTime
) ENGINE = MergeTree() ORDER BY (repo_id, tick);
```

### Fact Tables

```sql
-- Static analysis facts (per-function grain)
CREATE TABLE fact_function_complexity (
    repo_id              UInt64,
    source_file          String,
    directory            LowCardinality(String),
    language             LowCardinality(String),
    name                 String,
    cyclomatic_complexity UInt32,
    cognitive_complexity  UInt32,
    nesting_depth        UInt8,
    lines_of_code        UInt32,
    complexity_density   Float64,
    risk_level           LowCardinality(String)
) ENGINE = MergeTree()
ORDER BY (repo_id, directory, source_file, name);

-- Time-series facts (per-tick grain)
CREATE TABLE fact_tick_sentiment (
    repo_id        UInt64,
    tick           UInt32,
    start_time     DateTime,
    end_time       DateTime,
    sentiment      Float32,
    classification LowCardinality(String),
    comment_count  UInt32,
    commit_count   UInt32
) ENGINE = MergeTree()
ORDER BY (repo_id, tick);

-- Developer activity (per-tick-per-developer grain)
CREATE TABLE fact_developer_activity (
    repo_id  UInt64,
    tick     UInt32,
    dev_id   UInt32,
    commits  UInt32
) ENGINE = MergeTree()
ORDER BY (repo_id, tick, dev_id);

-- File coupling (per-pair grain)
CREATE TABLE fact_file_coupling (
    repo_id           UInt64,
    file1             String,
    file2             String,
    co_changes        UInt32,
    coupling_strength Float64
) ENGINE = MergeTree()
ORDER BY (repo_id, file1, file2);
```

---

## ETL Pipeline

### Python (with dbt or standalone)

```python
import json

with open('report.json') as f:
    data = json.load(f)

# Extract metadata
meta = data['metadata']
repo_id = hash(meta['repo_path'])  # or use a sequence

# Extract analyzers by ID
analyzers = {a['id']: a['report'] for a in data['analyzers']}

# Load function complexity
functions = analyzers['static/complexity']['function_complexity']
# Each record already has: name, source_file, language, directory,
# cyclomatic_complexity, cognitive_complexity, etc.

# Load time-series with timestamps
sentiment_ts = analyzers['history/sentiment']['time_series']
# Each tick has: tick, start_time, end_time, sentiment, classification, ...

# Load developers
developers = analyzers['history/devs']['developers']
# Each has: id, name, email, commits, lines_added, languages (array), ...

# Load file coupling (can be millions of rows)
coupling = analyzers['history/couples']['file_coupling']
# Each has: file1, file2, co_changes, coupling_strength
```

### ClickHouse Direct Load

```bash
# Extract function complexity from NDJSON
grep '"static/complexity"' report.ndjson \
  | jq -c '.report.function_complexity[]' \
  | clickhouse-client --query "INSERT INTO fact_function_complexity FORMAT JSONEachRow"

# Extract sentiment time-series
grep '"history/sentiment"' report.ndjson \
  | jq -c '.report.time_series[]' \
  | clickhouse-client --query "INSERT INTO fact_tick_sentiment FORMAT JSONEachRow"
```

---

## Recommended Analyzer Selection

Not all 17 analyzers are needed for every use case. Select based on your
dashboard needs:

### Code Quality Dashboard

```bash
codefang run \
  -a static/complexity,static/halstead,static/cohesion,static/comments \
  -a history/quality \
  --format json --per-file /repo
```

**Produces**: Function-level metrics, quality trend over time.
**Row count**: ~200K functions + ~4K tick entries for a medium repo.

### Developer Analytics Dashboard

```bash
codefang run \
  -a history/devs,history/couples,history/sentiment \
  --format json /repo
```

**Produces**: Developer profiles, coupling networks, sentiment trends.
**Row count**: ~500 developers + ~5K coupling pairs + ~4K ticks.

### File Health Dashboard

```bash
codefang run \
  -a static/complexity,static/clones \
  -a history/file-history,history/couples \
  --format json --per-file /repo
```

**Produces**: Per-file complexity, churn hotspots, coupling networks.
**Row count**: ~30K files + ~100K coupling pairs.

### Full Analysis (Everything)

```bash
codefang run --format ndjson --per-file --memory-budget 8GB /repo
```

**Produces**: All 17 analyzers. Use NDJSON for large repos.

---

## Performance Tuning

### Static Analysis Workers

Control parallelism for the UAST parsing phase:

```bash
# Use all CPUs (default: min(NumCPU, 8))
codefang run --static-workers 16 --format json /repo
```

More workers = faster static phase but higher peak memory.

### History Analysis

The streaming pipeline auto-tunes chunk sizes based on `--memory-budget`.
No manual tuning needed. Key parameters:

| Parameter | Flag | Default | Effect |
|-----------|------|---------|--------|
| Memory budget | `--memory-budget` | 2GB | Controls chunk size |
| Commit limit | `--limit` | 0 (all) | Bounds history depth |
| First parent | `--first-parent` | false | Skip merge commits |
| Since | `--since` | none | Time-based filtering |

```bash
# Analyze only last 6 months, first-parent only
codefang run --since 6m --first-parent --format json /repo
```

---

## Row Count Estimates

Use these to plan DWH capacity:

| Table | Per 1K Files | Per 10K Commits | Per 50K Files |
|-------|-------------|-----------------|---------------|
| function_complexity | ~5K | — | ~150K |
| comment_quality | ~17K | — | ~500K |
| file_coupling | — | ~30K | ~4M |
| developer_activity | — | ~3K ticks * devs | ~15K |
| node_coupling | — | ~40K | ~1.5M |

**Storage**: ~2GB JSON for 50K files + 56K commits (kubernetes scale).
Compressed in ClickHouse: ~200MB.

---

## Materialized Views

Pre-aggregate for common dashboard queries:

```sql
-- Complexity by directory (for treemap)
CREATE MATERIALIZED VIEW mv_complexity_by_directory
ENGINE = AggregatingMergeTree() ORDER BY (repo_id, directory)
AS SELECT
    repo_id,
    directory,
    avg(cyclomatic_complexity) AS avg_complexity,
    max(cyclomatic_complexity) AS max_complexity,
    count() AS function_count,
    countIf(risk_level = 'CRITICAL') AS critical_count
FROM fact_function_complexity
GROUP BY repo_id, directory;

-- Sentiment trend (for time-series chart)
CREATE MATERIALIZED VIEW mv_sentiment_weekly
ENGINE = AggregatingMergeTree() ORDER BY (repo_id, week)
AS SELECT
    repo_id,
    toMonday(start_time) AS week,
    avg(sentiment) AS avg_sentiment,
    sum(comment_count) AS total_comments
FROM fact_tick_sentiment
GROUP BY repo_id, week;
```

---

## Troubleshooting

### OOM Kills

**Symptom**: Process killed during history analysis.
**Fix**: Set `--memory-budget` explicitly.

```bash
# Check available RAM
free -h

# Set budget to ~25% of available RAM
codefang run --memory-budget 4GB --format ndjson /repo
```

### Empty History Analyzers

Some analyzers require specific conditions:

| Analyzer | Requirement |
|----------|-------------|
| `burndown` (developer/file survival) | Enable via config: `Burndown.TrackPeople: true`, `Burndown.TrackFiles: true` |
| `history/imports` | Requires UAST-enabled pipeline mode |
| `history/typos` | Requires UAST-enabled pipeline mode |

### Large File Coupling Tables

`file_coupling` can produce millions of rows for large repos. Filter in your
ETL:

```python
# Only keep strong couplings
strong = [p for p in coupling if p['coupling_strength'] > 0.3]
```

Or limit at query time:

```sql
SELECT * FROM fact_file_coupling
WHERE coupling_strength > 0.3
ORDER BY coupling_strength DESC
LIMIT 1000;
```

### Missing Language/Directory on Some Records

The `language` and `directory` fields are populated by the UAST parser. If a
file's language is not supported by the parser, these fields will be empty.
Supported languages include Go, Python, Java, JavaScript, TypeScript, C, C++,
Ruby, Rust, and 40+ others.

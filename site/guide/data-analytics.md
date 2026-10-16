# Data analytics & DWH integration

Codefang produces richly structured JSON output designed for loading into
columnar data warehouses (ClickHouse, Greenplum, BigQuery, Snowflake) and
building BI dashboards. This guide covers the optimal pipeline from
repository analysis to production dashboards.

---

## Quick start

Analyze a repository and produce DWH-ready output:

```bash
# JSON for small-to-medium repos (< 5K files, < 10K commits)
codefang run --format json --per-file /path/to/repo > report.json

# NDJSON for large repos (one line per analyzer)
codefang run --format ndjson /path/to/repo > report.ndjson

# Limit history depth for faster iteration
codefang run --format json --per-file --limit 5000 /path/to/repo > report.json
```

---

## Output format selection

| Repo Size | Recommended Format | Reason |
|-----------|-------------------|--------|
| < 1K files | `json` | Small file, easy to inspect |
| 1K-10K files | `json` | Manageable (< 500MB typically) |
| 10K-50K files | `ndjson` | JSON gets multi-GB; NDJSON streams |
| 50K+ files | `ndjson` + `--limit` | Bound history for practical runtimes |

### JSON format

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

### NDJSON format

```bash
codefang run --format ndjson --per-file /repo > report.ndjson
```

One JSON line per analyzer. First line is metadata:

```json
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

## Bounding a history run

`--memory-budget` (default: half the RAM, at most 2 GiB) bounds what the
history walks hold in flight: each walk plans its chunk size and worker count
from its share, and `-v` prints the plan. The output never depends on the
budget. The analyzers' own aggregates still grow with the commit window, so
on a very large repository also bound the window:

| Flag | Effect |
|------|--------|
| `--limit N` | Only the N newest commits |
| `--since <time or revision>` | Only commits authored at/after the cutoff (`4380h`, `2025-01-01`, `v1.2.0`) |
| `--first-parent` | Skip the side branches of merges |
| `--max-changes-per-commit N` | Drop commits whose tree diff exceeds N changes (default 10000) — vendoring dumps, mass renames |
| `--languages go,python` | Only changes in these languages reach the analyzers |
| `--head` | The single HEAD commit |

Measured on the hercules repository, `codefang run -a 'history/*' --limit 500`
peaks at 752 MB resident memory and finishes in 4.7 s wall time.

```bash
# The newest 5000 commits of a kubernetes-sized repository
codefang run --format ndjson --limit 5000 ~/sources/kubernetes > report.ndjson
```

---

## Commit limiting

Use `--limit N` to analyze only the most recent N commits. This is useful for:

- **Fast iteration**: Test your ETL pipeline on a subset before running full history
- **Incremental analysis**: Analyze only recent changes for daily dashboards
- **Memory control**: Fewer commits = less memory, faster processing

```bash
# Last 1000 commits
codefang run --format json --per-file --limit 1000 /repo > recent.json

# Last 10000 commits
codefang run --format json --per-file --limit 10000 /repo > report.json

# Full history
codefang run --format json --per-file /repo > full.json
```

---

## Key fields for analytics

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

## Schema manifest

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

## Star schema design

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

### Fact tables

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

## ETL pipeline

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

### ClickHouse direct load

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

## Recommended analyzer selection

Not all 17 analyzers are needed for every use case. Select based on your
dashboard needs:

### Code quality dashboard

```bash
codefang run \
  -a static/complexity,static/halstead,static/cohesion,static/comments \
  -a history/quality \
  --format json --per-file /repo
```

**Produces**: Function-level metrics, quality trend over time.
**Row count**: ~200K functions + ~4K tick entries for a medium repo.

### Developer analytics dashboard

```bash
codefang run \
  -a history/devs,history/couples,history/sentiment \
  --format json /repo
```

**Produces**: Developer profiles, coupling networks, sentiment trends.
**Row count**: ~500 developers + ~5K coupling pairs + ~4K ticks.

### File health dashboard

```bash
codefang run \
  -a static/complexity,static/clones \
  -a history/file-history,history/couples \
  --format json --per-file /repo
```

**Produces**: Per-file complexity, churn hotspots, coupling networks.
**Row count**: ~30K files + ~100K coupling pairs.

### Full analysis (everything)

```bash
codefang run --format ndjson /repo
```

**Produces**: All 17 analyzers. Use NDJSON for large repos.

---

## Performance tuning

### Static analysis

The static phase is one walk of the working tree shared by every `static/*`
analyzer; each admitted file is read once. Narrow it with `--languages` and
`--extra-excluded-prefixes`; vendored and generated files are excluded by
default (`--include-vendored` / `--include-generated` re-include them).

### History analysis

The history phase has no tuning flags beyond the commit window (see
[Bounding a history run](#bounding-a-history-run)):

| Parameter | Flag | Default | Effect |
|-----------|------|---------|--------|
| Commit limit | `--limit` | 0 (all) | Analyze at most N commits — the N **newest** |
| First parent | `--first-parent` | false | Skip merge side branches |
| Since | `--since` | none | Time- or revision-based filtering |
| Change cap | `--max-changes-per-commit` | 0 (= 10000) | Drop oversized commits |

```bash
# Analyze only the last 6 months (4380 h), first-parent only
codefang run --since 4380h --first-parent --format json /repo

# ... or since a release tag, capped at the 500 newest commits in the window
codefang run --since v1.28.0 --limit 500 --format json /repo
```

Duration units are `h`/`m`/`s` only — there is no `d`/`w`, and `m` is
**minutes**, so `--since 6m` means the last six minutes, not six months.

!!! note "`--since` with inactive repos"
    If no commits fall within the `--since` window, history analyzers produce
    empty results (zero ticks, zero developers) and the run still exits 0.
    Static analyzers still run normally since they analyze the current file
    tree, not commit history.

---

## Recurring loads

There is no incremental cache and no checkpoint: every run analyzes the whole
requested window. For a recurring (for example daily) load, bound each run to
the commits that are new since the previous one with `--since`, and load the
static snapshot separately:

```bash
#!/bin/bash
REPO=/opt/repos/kubernetes
OUTPUT_DIR=/var/lib/codefang/output
DAY=$(date +%Y%m%d)

# Pull latest
cd "$REPO" && git pull --ff-only

# Static snapshot of the working tree
codefang run -a 'static/*' --format json --per-file \
  -o "$OUTPUT_DIR/static-$DAY.json" "$REPO"

# History of the last 24 hours, one line per commit and analyzer
codefang run -a 'history/*' --format ndjson --since 24h \
  -o "$OUTPUT_DIR/history-$DAY.ndjson" "$REPO"

# Load into ClickHouse
clickhouse-client --query "INSERT INTO codefang_raw FORMAT JSONEachRow" \
  < "$OUTPUT_DIR/history-$DAY.ndjson"
```

A window-bounded history run starts from the tree of the commit just before
the window, so a commit's lines added / removed are the lines it changed, as
in a full-history run. Whole-history aggregates (totals, bus factor, coupling
strength) describe the window only, and burndown counts the lines that
existed before the window as its oldest band.

---

## Row count estimates

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

## Materialized views

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

### High memory on long histories

**Symptom**: the process is killed or swaps during history analysis.
**Fix**: bound the window — `--limit`, `--since`, `--first-parent`,
`--max-changes-per-commit`, `--languages`.

```bash
# The newest 10000 commits only
codefang run --limit 10000 --format ndjson /repo
```

### Empty history analyzers

Some analyzers require specific conditions:

| Analyzer | Requirement |
|----------|-------------|
| `burndown` (developer/file survival) | `--burndown-people` / `--burndown-files` (config: `history.burndown.track_people` / `track_files`) |
| `history/imports` | Only files in a language the UAST parser supports contribute |
| `history/typos` | Only files in a language the UAST parser supports contribute |

### Large file coupling tables

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

### Missing language/directory on some records

The `language` and `directory` fields are populated by the UAST parser. If a
file's language is not supported by the parser, these fields will be empty.
Supported languages include Go, Python, Java, JavaScript, TypeScript, C, C++,
Ruby, Rust, and 40+ others.

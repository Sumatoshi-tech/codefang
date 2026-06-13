# cf-plotpage

The `--format plot` HTML-page renderer for codefang, with the go-echarts
option-model subset it renders charts through.

## What and why

`codefang ... --format plot` emits a self-contained HTML page of charts for an
analyzer's results. The page bytes are a frozen contract: they must match the
reference binary's output byte for byte, modulo chart element IDs. go-echarts
draws a random 12-char `[A-Za-z]` ID per chart, the only run-to-run
nondeterminism in a reference page; this crate replaces that with the
deterministic [`ChartIdGen`] sequence so two runs are byte-identical end to end.

All chart-option JSON routes through `cf-gojson` (never `serde`): it is
report-contract output, byte-compared against the reference binary by
`rust/tests/compat`.

## Usage

Build charts, assemble them into sections of a `Page`, and render:

```rust
use cf_plotpage::{build_bar_chart, BarSeries, SeriesValue, Hint, Page, Section};

let labels = vec!["a".to_string(), "b".to_string()];
let series = vec![BarSeries {
    name: "count".to_string(),
    data: vec![SeriesValue::Int(3), SeriesValue::Int(7)],
    ..BarSeries::default()
}];
let chart = build_bar_chart(None, &labels, &series, "items");

let mut page = Page::new("static/complexity", "demo");
page.add(vec![Section::new(
    "Top files",
    "by complexity",
    Box::new(chart),
    Hint::default(),
)]);

let html = page.render();
assert!(html.starts_with("<!doctype html>"));
```

This example is the crate-level doctest in `src/lib.rs`, run by
`cargo test --doc -p cf-plotpage`.

## Build

```sh
cargo build -p cf-plotpage
cargo test -p cf-plotpage
```

## Deeper docs

See the crate rustdoc (`cargo doc -p cf-plotpage --open`) — module-by-module
notes on the go-echarts model (`echarts`), themes (`theme`), components
(`components`), and the page shell (`page`, `templates`).

[`ChartIdGen`]: https://docs.rs/cf-plotpage

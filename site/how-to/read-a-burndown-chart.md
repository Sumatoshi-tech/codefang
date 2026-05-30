# How to read a burndown chart

<!-- Template: Good Docs Project how-to (CC-BY 4.0) — https://github.com/thegooddocsproject/templates -->

## Goal

Generate a code-survival burndown for a repository and interpret the result so
you can see how much code from each era still survives at the latest sample.

## Prerequisites

- `codefang` installed and a Git repository checked out with full history.
- A web browser to view the interactive HTML chart.
- `jq` if you want to inspect the raw matrix.

## Steps

1. Run the burndown analyzer and write the YAML output so you can read the raw
   matrix:

   ```bash
   codefang run -a history/burndown --format yaml .
   ```

2. Read the `project` matrix. Each row is a sampling point in time; each value
   in a row is the number of lines, created in a given age band, that still
   survive at that sample. As you read down the rows, watch a band's values
   shrink — that is code being rewritten or deleted over time:

   ```yaml
   burndown:
     granularity: 30
     sampling: 30
     project:
       - [0, 1200, 1100, 980, 920, 870]
       - [0, 0, 320, 290, 270, 250]
     ticks:
       - "2024-01-15"
       - "2024-02-14"
   ```

3. Use `ticks` to map matrix positions back to calendar dates. The `granularity`
   key is the width of each age band (in ticks) and `sampling` is how often a
   snapshot is taken; the default tick is a 24-hour window.

4. Add a per-developer breakdown to see whose code survives, by enabling
   developer tracking:

   ```bash
   codefang run -a history/burndown --burndown-people --format yaml .
   ```

5. Generate the interactive stacked-area chart and open it in a browser. This is
   the visual form of the same matrix:

   ```bash
   codefang run -a history/burndown --format plot . > burndown.html
   xdg-open burndown.html   # Linux
   open burndown.html       # macOS
   ```

6. Extract just the tick axis for scripting or a custom plot:

   ```bash
   codefang run -a history/burndown --format json . | jq '.burndown.ticks'
   ```

## Result

You can point at any value in the chart or matrix and state what it means: "this
many lines, written around this date, still survive at this later date." A
declining survival curve signals churn; a flat top band signals stable,
long-lived code.

## See also

- [Understanding code burndown](../explanation/burndown.md) — the mental model.
- [Burndown reference](../analyzers/burndown.md) — configuration keys and output schema.
- [How to run Codefang in CI](run-in-ci.md)

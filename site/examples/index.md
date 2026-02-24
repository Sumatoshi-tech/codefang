# Examples

All examples below are generated from the [Kubernetes](https://github.com/kubernetes/kubernetes) repository — one of the largest open-source Go projects with 135k+ commits and thousands of contributors.

## History Analyzers

### Burndown

The burndown chart shows how code survives over time: each colored band represents code written in a specific period, and its height shows how many lines from that period are still present at each sample point.

<div id="kubernetes-burndown-chart" style="width: 100%; height: 600px;"></div>
<script src="https://go-echarts.github.io/go-echarts-assets/assets/echarts.min.js"></script>
<script src="kubernetes-burndown-data.js"></script>
<script>
(function() {
    var chart = echarts.init(document.getElementById('kubernetes-burndown-chart'), null, { renderer: 'canvas' });
    chart.setOption(burndownChartOption);
    window.addEventListener('resize', function() { chart.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Stacked layers** = code written in different time periods
    - **Bottom layers** = oldest code still surviving
    - **Narrowing layers** = code being deleted or rewritten
    - **Flat layers** = stable code that rarely changes

```bash
codefang run -a history/burndown --format plot /path/to/repo > burndown.html
```

See the [Burndown analyzer docs](../analyzers/burndown.md) for configuration options.

---

### Couples

The couples chart visualizes co-change relationships between files — files that are frequently modified together in the same commits.

<div id="kubernetes-couples-chart" style="width: 100%; height: 600px;"></div>
<script src="kubernetes-couples-data.js"></script>
<script>
(function() {
    var chart = echarts.init(document.getElementById('kubernetes-couples-chart'), null, { renderer: 'canvas' });
    chart.setOption(couplesChartOption);
    window.addEventListener('resize', function() { chart.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Connected nodes** = files frequently changed together
    - **Cluster density** = tightly coupled modules
    - **Isolated nodes** = independent components

```bash
codefang run -a history/couples --format plot /path/to/repo > couples.html
```

See the [Couples analyzer docs](../analyzers/couples.md) for configuration options.

---

### File History

The file history chart shows the most frequently modified files across the repository's entire commit history.

<div id="kubernetes-file-history-chart" style="width: 100%; height: 600px;"></div>
<script src="kubernetes-file-history-data.js"></script>
<script>
(function() {
    var chart = echarts.init(document.getElementById('kubernetes-file-history-chart'), null, { renderer: 'canvas' });
    chart.setOption(filehistoryChartOption);
    window.addEventListener('resize', function() { chart.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Tallest bars** = most frequently changed files (hotspots)
    - High churn files often benefit from refactoring or better test coverage

```bash
codefang run -a history/file-history --format plot /path/to/repo > file-history.html
```

See the [File History analyzer docs](../analyzers/file-history.md) for configuration options.

---

### Anomaly Detection

The anomaly chart detects unusual patterns in commit activity — spikes or drops that deviate significantly from the repository's normal rhythm.

<div id="kubernetes-anomaly-chart" style="width: 100%; height: 600px;"></div>
<script src="kubernetes-anomaly-data.js"></script>
<script>
(function() {
    var chart = echarts.init(document.getElementById('kubernetes-anomaly-chart'), null, { renderer: 'canvas' });
    chart.setOption(anomalyChartOption);
    window.addEventListener('resize', function() { chart.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Red points** = anomalous activity (unusually high or low)
    - **Blue line** = normal commit activity trend
    - Anomalies may indicate major refactors, releases, or team changes

```bash
codefang run -a history/anomaly --format plot /path/to/repo > anomaly.html
```

See the [Anomaly Detection analyzer docs](../analyzers/anomaly.md) for configuration options.

---

### Imports (History)

The imports history chart tracks how import dependencies evolve over the repository's commit history.

<div id="kubernetes-imports-history-chart" style="width: 100%; height: 600px;"></div>
<script src="kubernetes-imports-history-data.js"></script>
<script>
(function() {
    var chart = echarts.init(document.getElementById('kubernetes-imports-history-chart'), null, { renderer: 'canvas' });
    chart.setOption(importshistoryChartOption);
    window.addEventListener('resize', function() { chart.resize(); });
})();
</script>

```bash
codefang run -a history/imports --format plot /path/to/repo > imports-history.html
```

---

### Developers

The developer dashboard shows contributor activity, workload distribution, language expertise, and code churn across the project's history.

<div id="kubernetes-devs-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-devs-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<div id="kubernetes-devs-chart2" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<div id="kubernetes-devs-chart3" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-devs-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-devs-chart0'), null, { renderer: 'canvas' });
    c0.setOption(devsChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-devs-chart1'), null, { renderer: 'canvas' });
    c1.setOption(devsChart1);
    var c2 = echarts.init(document.getElementById('kubernetes-devs-chart2'), null, { renderer: 'canvas' });
    c2.setOption(devsChart2);
    var c3 = echarts.init(document.getElementById('kubernetes-devs-chart3'), null, { renderer: 'canvas' });
    c3.setOption(devsChart3);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); c2.resize(); c3.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Activity chart** = commit frequency per developer over time
    - **Workload treemap** = proportional contribution by developer
    - **Radar chart** = language expertise across top contributors
    - **Code churn** = lines added vs removed per time period

```bash
codefang run -a history/devs --format plot /path/to/repo > devs.html
```

See the [Developers analyzer docs](../analyzers/developers.md) for configuration options.

---

### Sentiment

The sentiment chart shows the emotional tone of commit messages over the project's history, detecting periods of positive or negative developer sentiment.

<div id="kubernetes-sentiment-chart" style="width: 100%; height: 600px;"></div>
<script src="kubernetes-sentiment-data.js"></script>
<script>
(function() {
    var chart = echarts.init(document.getElementById('kubernetes-sentiment-chart'), null, { renderer: 'canvas' });
    chart.setOption(sentimentChartOption);
    window.addEventListener('resize', function() { chart.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Positive values** = constructive, positive commit messages
    - **Negative values** = frustration, urgency, or negative sentiment
    - **Sudden drops** = may indicate stressful periods or difficult bugs

```bash
codefang run -a history/sentiment --format plot /path/to/repo > sentiment.html
```

See the [Sentiment analyzer docs](../analyzers/sentiment.md) for configuration options.

---

### Shotness

The shotness charts visualize function-level co-change patterns — which functions are frequently modified together across commits.

<div id="kubernetes-shotness-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-shotness-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<div id="kubernetes-shotness-chart2" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-shotness-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-shotness-chart0'), null, { renderer: 'canvas' });
    c0.setOption(shotnessChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-shotness-chart1'), null, { renderer: 'canvas' });
    c1.setOption(shotnessChart1);
    var c2 = echarts.init(document.getElementById('kubernetes-shotness-chart2'), null, { renderer: 'canvas' });
    c2.setOption(shotnessChart2);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); c2.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Treemap** = function-level change frequency (larger = more changes)
    - **Heatmap** = co-change correlation between functions
    - **Bar chart** = most frequently modified functions

```bash
codefang run -a history/shotness --format plot /path/to/repo > shotness.html
```

See the [Shotness analyzer docs](../analyzers/shotness.md) for configuration options.

---

### Typos

The typos chart detects potential typos and misspellings in identifiers across the codebase's commit history.

<div id="kubernetes-typos-chart" style="width: 100%; height: 600px;"></div>
<script src="kubernetes-typos-data.js"></script>
<script>
(function() {
    var chart = echarts.init(document.getElementById('kubernetes-typos-chart'), null, { renderer: 'canvas' });
    chart.setOption(typosChartOption);
    window.addEventListener('resize', function() { chart.resize(); });
})();
</script>

```bash
codefang run -a history/typos --format plot /path/to/repo > typos.html
```

See the [Typos analyzer docs](../analyzers/typos.md) for configuration options.

---

### Quality

The quality charts track code quality metrics (complexity and Halstead volume) over the commit history, showing how code quality evolves.

<div id="kubernetes-quality-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-quality-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-quality-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-quality-chart0'), null, { renderer: 'canvas' });
    c0.setOption(qualityChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-quality-chart1'), null, { renderer: 'canvas' });
    c1.setOption(qualityChart1);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Complexity trend** = how function complexity changes over time
    - **Halstead volume** = information content evolution of the codebase

```bash
codefang run -a history/quality --format plot /path/to/repo > quality.html
```

---

## Static Analyzers

### Complexity

The complexity charts show cyclomatic and cognitive complexity metrics for the most complex functions in the codebase.

<div id="kubernetes-complexity-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-complexity-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<div id="kubernetes-complexity-chart2" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-complexity-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-complexity-chart0'), null, { renderer: 'canvas' });
    c0.setOption(complexityChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-complexity-chart1'), null, { renderer: 'canvas' });
    c1.setOption(complexityChart1);
    var c2 = echarts.init(document.getElementById('kubernetes-complexity-chart2'), null, { renderer: 'canvas' });
    c2.setOption(complexityChart2);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); c2.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Cyclomatic complexity** = number of linearly independent paths through the code
    - **Cognitive complexity** = how difficult the code is for humans to understand
    - Functions with high complexity are candidates for refactoring

```bash
codefang run -a static/complexity --format plot /path/to/repo > complexity.html
```

See the [Complexity analyzer docs](../analyzers/complexity.md) for configuration options.

---

### Halstead

The Halstead charts measure software complexity through operator/operand counting — effort, difficulty, and volume metrics.

<div id="kubernetes-halstead-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-halstead-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<div id="kubernetes-halstead-chart2" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-halstead-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-halstead-chart0'), null, { renderer: 'canvas' });
    c0.setOption(halsteadChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-halstead-chart1'), null, { renderer: 'canvas' });
    c1.setOption(halsteadChart1);
    var c2 = echarts.init(document.getElementById('kubernetes-halstead-chart2'), null, { renderer: 'canvas' });
    c2.setOption(halsteadChart2);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); c2.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Effort** = estimated mental effort to understand the code
    - **Difficulty** = how error-prone the code is
    - **Volume** = information content of the code

```bash
codefang run -a static/halstead --format plot /path/to/repo > halstead.html
```

See the [Halstead analyzer docs](../analyzers/halstead.md) for configuration options.

---

### Cohesion

The cohesion charts measure how well the methods within a class or struct relate to each other. Higher cohesion indicates better-designed components.

<div id="kubernetes-cohesion-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-cohesion-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-cohesion-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-cohesion-chart0'), null, { renderer: 'canvas' });
    c0.setOption(cohesionChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-cohesion-chart1'), null, { renderer: 'canvas' });
    c1.setOption(cohesionChart1);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Score of 1.0** = all methods use all fields (perfect cohesion)
    - **Score near 0** = methods don't share fields (low cohesion, consider splitting)

```bash
codefang run -a static/cohesion --format plot /path/to/repo > cohesion.html
```

See the [Cohesion analyzer docs](../analyzers/cohesion.md) for configuration options.

---

### Comments

The comments charts analyze documentation coverage — comment density, lines of code, and documentation scores across the codebase.

<div id="kubernetes-comments-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-comments-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<div id="kubernetes-comments-chart2" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-comments-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-comments-chart0'), null, { renderer: 'canvas' });
    c0.setOption(commentsChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-comments-chart1'), null, { renderer: 'canvas' });
    c1.setOption(commentsChart1);
    var c2 = echarts.init(document.getElementById('kubernetes-comments-chart2'), null, { renderer: 'canvas' });
    c2.setOption(commentsChart2);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); c2.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Documentation score** = ratio of documented public symbols
    - **Lines of code** = file sizes across the codebase
    - Files with low scores and high LOC are documentation priorities

```bash
codefang run -a static/comments --format plot /path/to/repo > comments.html
```

See the [Comments analyzer docs](../analyzers/comments.md) for configuration options.

---

### Imports (Static)

The static imports charts show the most used import packages and their categorization across the codebase.

<div id="kubernetes-imports-static-chart0" style="width: 100%; height: 500px;"></div>
<div id="kubernetes-imports-static-chart1" style="width: 100%; height: 500px; margin-top: 20px;"></div>
<script src="kubernetes-imports-static-data.js"></script>
<script>
(function() {
    var c0 = echarts.init(document.getElementById('kubernetes-imports-static-chart0'), null, { renderer: 'canvas' });
    c0.setOption(importsstaticChart0);
    var c1 = echarts.init(document.getElementById('kubernetes-imports-static-chart1'), null, { renderer: 'canvas' });
    c1.setOption(importsstaticChart1);
    window.addEventListener('resize', function() { c0.resize(); c1.resize(); });
})();
</script>

!!! tip "How to interpret"

    - **Usage count** = how many files import each package
    - **Categories** = standard library, internal, external dependencies

```bash
codefang run -a static/imports --format plot /path/to/repo > imports.html
```

See the [Imports analyzer docs](../analyzers/imports.md) for configuration options.

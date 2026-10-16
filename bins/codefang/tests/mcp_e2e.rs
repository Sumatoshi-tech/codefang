//! End-to-end coverage of `codefang mcp`, the Model Context Protocol server,
//! driven over its stdio transport through the **real binary**
//! (`site/integrations/mcp.md`): the client handshake, the tool list, and
//! one call of each tool, whose results are the CLI's own reports.

mod common;

use std::collections::BTreeMap;
use std::io::Write as _;
use std::process::Stdio;

use cf_gitlib::testutil::{commit_files_as, init_repo, TestRepo};
use serde_json::{json, Value};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day.
const DAY: i64 = 86_400;
/// A Go function with one branch.
const GO_CODE: &str =
    "package main\n\nfunc sign(x int) int {\n\tif x < 0 {\n\t\treturn -1\n\t}\n\treturn 1\n}\n";

/// A two-author history.
fn history() -> TestRepo {
    let repo = init_repo().expect("init");
    commit_files_as(
        &repo,
        "alice",
        "alice@x",
        "add",
        T0,
        &[("a.go", GO_CODE.as_bytes())],
    )
    .expect("commit");
    let edited = format!("{GO_CODE}\nfunc two() int {{\n\treturn 2\n}}\n");
    commit_files_as(
        &repo,
        "bob",
        "bob@x",
        "edit",
        T0 + DAY,
        &[("a.go", edited.as_bytes())],
    )
    .expect("commit");
    repo
}

/// Sends `requests` (one JSON-RPC message per line) to `codefang mcp`, closes
/// its stdin and returns every response keyed by request id.
fn session(requests: &[Value]) -> BTreeMap<i64, Value> {
    let mut child = common::command(BIN)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn codefang mcp");
    {
        let mut stdin = child.stdin.take().expect("stdin");
        for request in requests {
            writeln!(stdin, "{request}").expect("write request");
        }
    }
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "mcp exited {:?}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf-8")
        .lines()
        .map(|line| {
            let response: Value =
                serde_json::from_str(line).expect("one JSON-RPC response per line");
            (response["id"].as_i64().expect("response id"), response)
        })
        .collect()
}

/// A `tools/call` request.
fn call(id: i64, tool: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
           "params": {"name": tool, "arguments": arguments}})
}

/// The JSON document a successful tool result carries.
fn tool_json(response: &Value) -> Value {
    let result = &response["result"];
    assert_eq!(result["isError"], false, "{response}");
    let text = result["content"][0]["text"].as_str().expect("text content");
    serde_json::from_str(text).unwrap_or_else(|e| panic!("tool text is not JSON ({e}): {text}"))
}

/// The handshake, the tool list, and one call of every tool: the parsed
/// tree, the static report and the history report the CLI renders.
#[test]
fn an_mcp_client_lists_and_calls_every_tool() {
    let repo = history();
    let repo_path = repo.path().to_str().expect("utf-8 path");
    let responses = session(&[
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
               "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                          "clientInfo": {"name": "e2e", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        call(3, "uast_parse", json!({"code": GO_CODE, "language": "go"})),
        call(
            4,
            "codefang_analyze",
            json!({"code": GO_CODE, "language": "go", "analyzers": ["complexity"]}),
        ),
        call(
            5,
            "codefang_history",
            json!({"repo_path": repo_path, "analyzers": ["devs"]}),
        ),
        call(6, "codefang_analyze", json!({"code": "", "language": "go"})),
    ]);

    assert!(
        responses[&1]["result"]["serverInfo"]["name"].is_string(),
        "{}",
        responses[&1]
    );
    let mut tools: Vec<&str> = responses[&2]["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("tool name"))
        .collect();
    tools.sort_unstable();
    assert_eq!(
        tools,
        ["codefang_analyze", "codefang_history", "uast_parse"]
    );

    let tree = serde_json::to_string(&tool_json(&responses[&3])).expect("tree");
    assert!(
        tree.contains("sign"),
        "the parsed tree names the function: {tree}"
    );

    let analyzed = tool_json(&responses[&4]);
    let cli = common::command(BIN)
        .args(["run", "-a", "static/complexity", "--format", "json"])
        .arg(write_snippet(GO_CODE).path())
        .output()
        .expect("run");
    let cli: Value = serde_json::from_slice(&cli.stdout).expect("cli json");
    assert_eq!(
        analyzed["complexity"], cli,
        "the analyze tool returns the CLI report"
    );

    let history = tool_json(&responses[&5]);
    let cli = common::command(BIN)
        .args([
            "run",
            "-a",
            "history/devs",
            "--format",
            "json",
            "--limit",
            "1000",
            repo_path,
        ])
        .output()
        .expect("run");
    let cli: Value = serde_json::from_slice(&cli.stdout).expect("cli json");
    assert_eq!(
        history["devs"], cli,
        "the history tool returns the CLI report"
    );

    assert_eq!(
        responses[&6]["result"]["isError"], true,
        "{}",
        responses[&6]
    );
}

/// A directory holding `code` as `code.go`, the file the analyze tool reads.
fn write_snippet(code: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("code.go"), code).expect("write");
    dir
}

/// The tools take Linguist language names (and their aliases), not only
/// file extensions.
#[test]
fn tools_take_language_names() {
    let responses = session(&[
        call(
            1,
            "uast_parse",
            json!({"code": "def hello():\n    return 1\n", "language": "python"}),
        ),
        call(
            2,
            "uast_parse",
            json!({"code": "function hello() { return 1 }\n", "language": "JavaScript"}),
        ),
        call(
            3,
            "uast_parse",
            json!({"code": "def hello():\n    return 1\n", "language": "py"}),
        ),
        call(
            4,
            "codefang_analyze",
            json!({"code": "def hello():\n    return 1\n", "language": "python", "analyzers": ["complexity"]}),
        ),
        call(
            5,
            "uast_parse",
            json!({"code": "x", "language": "notalang"}),
        ),
    ]);
    for id in 1..=4 {
        let tree = serde_json::to_string(&tool_json(&responses[&id])).expect("tree");
        assert!(tree.contains("hello"), "{id}: {tree}");
    }
    let unknown = &responses[&5]["result"];
    assert_eq!(unknown["isError"], true, "{unknown}");
    assert!(
        unknown["content"][0]["text"]
            .as_str()
            .is_some_and(|t| t.contains("unsupported language: notalang")),
        "{unknown}"
    );
}

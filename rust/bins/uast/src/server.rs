//! `uast server` — UAST development HTTP server (port of `cmd/uast/server.go`).
//!
//! Serves `/api/parse`, `/api/query`, `/api/mappings`, and `/api/mappings/<name>`
//! over HTTP. Go uses `net/http` + otel middleware; the otel/tracing layer is
//! behavioral-only and dropped here. Responses are serialized through
//! [`cf_textutil::write_json`] with `pretty=false` (compact, trailing newline) —
//! matching Go's `writeJSON(..., false)`. The embedded UAST/query result is a
//! JSON *string* produced with two-space indent, matching Go's
//! `json.MarshalIndent(..., "", "  ")`.
//!
//! Input request bodies are decoded with `serde_json` (DESIGN §2: input only).

use std::io::Write;

use clap::{Arg, ArgAction, ArgMatches, Command};
use cf_textutil::{GoMap, GoValue};
use cf_uast::Parser;
use serde_json::Value;
use tiny_http::{Header, Method, Response, Server};

use crate::govalue_bridge::node_to_value;
use crate::query;

/// Minimum URL path parts for a single-mapping request (server.go).
const MIN_MAPPING_URL_PARTS: usize = 3;

/// Builds the `server` subcommand (server.go:60-77).
pub fn command() -> Command {
    Command::new("server")
        .about("Start UAST development server")
        .long_about("Start a web server that provides UAST parsing and querying via HTTP API")
        .arg(
            Arg::new("port")
                .long("port")
                .short('p')
                .help("port to listen on")
                .default_value("8080")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("static")
                .long("static")
                .short('s')
                .help("directory to serve static files from")
                .default_value("")
                .action(ArgAction::Set),
        )
}

/// Runs `server` (server.go `startServer`). Blocks serving requests; never
/// returns under normal operation.
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let port = m.get_one::<String>("port").map(String::as_str).unwrap_or("8080");
    let static_dir = m.get_one::<String>("static").map(String::as_str).unwrap_or("");

    let addr = format!("0.0.0.0:{port}");
    let server = Server::http(&addr).map_err(|e| format!("server failed: {e}"))?;
    eprintln!("UAST Development Server starting addr=http://localhost:{port}");
    if !static_dir.is_empty() {
        eprintln!("serving static files dir={static_dir}");
    }

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);

        let response = route(&method, &url, &body);
        let _ = request.respond(response);
    }
    Ok(())
}

/// Routes a request to the matching API handler.
fn route(method: &Method, url: &str, body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    // Strip any query string for path matching.
    let path = url.split('?').next().unwrap_or(url);
    match path {
        "/api/parse" => {
            if *method != Method::Post {
                return text_response(405, "Method not allowed");
            }
            handle_parse(body)
        }
        "/api/query" => {
            if *method != Method::Post {
                return text_response(405, "Method not allowed");
            }
            handle_query(body)
        }
        "/api/mappings" => {
            if *method != Method::Get {
                return text_response(405, "Method not allowed");
            }
            handle_mappings_list()
        }
        p if p.starts_with("/api/mappings/") => {
            if *method != Method::Get {
                return text_response(405, "Method not allowed");
            }
            handle_mapping(p)
        }
        _ => text_response(404, "Not found"),
    }
}

/// Handles `POST /api/parse` (server.go `handleParse`). The response is
/// `{uast, error?}` where `uast` is the indented JSON string of the parsed tree.
fn handle_parse(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let req: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return text_response(400, "Invalid request body"),
    };
    let code = req.get("code").and_then(Value::as_str).unwrap_or("");
    let language = req.get("language").and_then(Value::as_str).unwrap_or("");

    let parser = Parser::new();
    let filename = format!("input.{}", file_extension(language));

    match parser.parse(&filename, code.as_bytes()) {
        Ok(mut node) => {
            node.assign_stable_ids();
            let value = node_to_value(&node);
            // Go marshals the UAST with two-space indent into a string field.
            let uast_str = String::from_utf8(
                cf_textutil::marshal_json(&value, true).unwrap_or_default(),
            )
            .unwrap_or_default();
            // marshal_json appends a trailing newline; Go's MarshalIndent does
            // not, so trim it for the embedded string.
            let uast_str = uast_str.trim_end_matches('\n').to_string();
            json_response(&GoValue::object(GoMap::from_map(vec![(
                "uast".to_string(),
                GoValue::Str(uast_str),
            )])))
        }
        Err(e) => json_response(&GoValue::object(GoMap::from_map(vec![(
            "error".to_string(),
            GoValue::Str(format!("Parse error: {e}")),
        )]))),
    }
}

/// Handles `POST /api/query` (server.go `handleQuery`).
fn handle_query(body: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let req: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return text_response(400, "Invalid request body"),
    };
    let uast = req.get("uast").and_then(Value::as_str).unwrap_or("");
    let query_str = req.get("query").and_then(Value::as_str).unwrap_or("");

    let node = match serde_json::from_str::<Value>(uast) {
        Ok(v) => query::json_to_node_public(&v),
        Err(e) => {
            return json_response(&GoValue::object(GoMap::from_map(vec![(
                "error".to_string(),
                GoValue::Str(format!("Failed to parse UAST JSON: {e}")),
            )])));
        }
    };

    match node.find_dsl(query_str) {
        Ok(results) => {
            let value = query::nodes_to_value_public(&results);
            let results_str = String::from_utf8(
                cf_textutil::marshal_json(&value, true).unwrap_or_default(),
            )
            .unwrap_or_default();
            let results_str = results_str.trim_end_matches('\n').to_string();
            json_response(&GoValue::object(GoMap::from_map(vec![(
                "results".to_string(),
                GoValue::Str(results_str),
            )])))
        }
        Err(e) => json_response(&GoValue::object(GoMap::from_map(vec![(
            "error".to_string(),
            GoValue::Str(format!("Query error: {e}")),
        )]))),
    }
}

/// Handles `GET /api/mappings` (server.go `handleGetMappingsList`).
fn handle_mappings_list() -> Response<std::io::Cursor<Vec<u8>>> {
    let parser = Parser::new();
    let list = parser.get_embedded_mappings_list();
    let entries: Vec<(String, GoValue)> = list
        .into_iter()
        .map(|(lang, info)| {
            (
                lang,
                GoValue::object(GoMap::from_map(vec![(
                    "size".to_string(),
                    GoValue::Int(info.size as i64),
                )])),
            )
        })
        .collect();
    json_response(&GoValue::object(GoMap::from_map(entries)))
}

/// Handles `GET /api/mappings/<name>` (server.go `handleGetMapping`).
fn handle_mapping(path: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < MIN_MAPPING_URL_PARTS {
        return text_response(400, "Invalid mapping path");
    }
    let name = parts[parts.len() - 1];
    let parser = Parser::new();
    match parser.get_mapping(name) {
        Ok(map) => {
            // *Map struct (server.go): declaration order uast, extensions.
            let mut m = GoMap::new_struct();
            m.push("uast", GoValue::Str(map.uast));
            m.push(
                "extensions",
                GoValue::Array(map.extensions.into_iter().map(GoValue::Str).collect()),
            );
            let value = GoValue::Object(m);
            json_response(&value)
        }
        Err(e) => text_response(404, &format!("Mapping not found: {e}")),
    }
}

/// Maps a language name to a file extension (server.go `getFileExtension`).
fn file_extension(language: &str) -> &'static str {
    match language.to_lowercase().as_str() {
        "go" => "go",
        "python" => "py",
        "javascript" => "js",
        "typescript" => "ts",
        "java" => "java",
        "cpp" => "cpp",
        "c" => "c",
        "rust" => "rs",
        "ruby" => "rb",
        "php" => "php",
        "csharp" => "cs",
        "kotlin" => "kt",
        "swift" => "swift",
        "scala" => "scala",
        "dart" => "dart",
        "lua" => "lua",
        "bash" => "sh",
        "html" => "html",
        "css" => "css",
        "json" => "json",
        "yaml" => "yaml",
        "xml" => "xml",
        "sql" => "sql",
        _ => "txt",
    }
}

/// Builds a compact-JSON response (Go `writeJSON(..., false)`), with the
/// `application/json` content type.
fn json_response(value: &GoValue) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut buf = Vec::new();
    let _ = cf_textutil::write_json(&mut buf, value, false);
    let header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static header");
    Response::from_data(buf).with_header(header)
}

/// Builds a plain-text response with a status code (Go `http.Error`).
fn text_response(status: u16, msg: &str) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut body = msg.as_bytes().to_vec();
    let _ = body.write_all(b"\n");
    Response::from_data(body).with_status_code(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_extension_mapping() {
        assert_eq!(file_extension("go"), "go");
        assert_eq!(file_extension("Python"), "py");
        assert_eq!(file_extension("unknown-lang"), "txt");
    }
}

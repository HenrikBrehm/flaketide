//! `flaketide mcp` — Model Context Protocol server over stdio.
//!
//! Lets agentic coding tools (Claude Code, Cursor, Continue, OpenCode, etc.)
//! query flaketide directly from inside an editor:
//!
//!     "Which tests are flaky right now?"
//!     -> Claude calls list_flaky_tests via MCP, returns a structured answer.
//!
//! Protocol: JSON-RPC 2.0 framed as newline-delimited JSON on stdin/stdout.
//! Spec: https://modelcontextprotocol.io
//!
//! Exposed tools:
//!   - list_flaky_tests       — all flake verdicts, sorted by severity
//!   - get_test_history       — runs for one test in the last N days
//!   - get_test_stats         — posterior, CI, severity for one test
//!   - get_quarantine_debt    — current quarantine list
//!   - analyze_test           — AI root-cause classification (requires API key)

use std::io::{BufRead, Write};
use std::path::Path;

use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ai::{ClassifyInput, Classifier};
use crate::config::load_config;
use crate::domain::TestId;
use crate::error::{FlaketideError, Result};
use crate::stats;
use crate::store::Store;
use crate::util::paths::find_repo_root;

#[derive(Debug, Args)]
pub struct McpArgs {}

pub async fn run(_args: McpArgs, config: Option<&Path>) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let (cfg, _) = load_config(config, &cwd)?;
    let root = find_repo_root(&cwd).unwrap_or(cwd);
    let db_path = if cfg.db_path.is_absolute() { cfg.db_path.clone() } else { root.join(&cfg.db_path) };
    let store = Store::open(&db_path).await?;
    let ai_cfg = cfg.ai.clone();
    let thr = cfg.thresholds.clone();

    eprintln!("flaketide mcp: speaking JSON-RPC 2.0 on stdio (Ctrl-D to exit)");

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() { continue; }
        let req: Result<JsonRpcRequest> = serde_json::from_str(&line)
            .map_err(|e| FlaketideError::Other(anyhow::anyhow!("bad JSON-RPC: {e}")));
        let resp = match req {
            Ok(r) => handle_request(r, &store, &ai_cfg, &thr).await,
            Err(e) => JsonRpcResponse::error(None, -32700, format!("parse error: {e}")),
        };
        writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
        stdout.flush()?;
    }
    Ok(0)
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

impl JsonRpcResponse {
    fn ok(id: Option<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0", id: id.unwrap_or(Value::Null), result: Some(result), error: None }
    }
    fn error(id: Option<Value>, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id: id.unwrap_or(Value::Null),
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

async fn handle_request(
    req: JsonRpcRequest,
    store: &Store,
    ai_cfg: &crate::domain::AiConfig,
    thr: &crate::domain::Thresholds,
) -> JsonRpcResponse {
    if req.jsonrpc != "2.0" {
        return JsonRpcResponse::error(req.id, -32600, "jsonrpc must be \"2.0\"".into());
    }
    match req.method.as_str() {
        "initialize" => JsonRpcResponse::ok(req.id, json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "flaketide", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": { "tools": {} }
        })),
        "tools/list" => JsonRpcResponse::ok(req.id, json!({
            "tools": [
                tool_descriptor("list_flaky_tests", "List every test that flaketide currently considers flaky, sorted by severity (highest first).", json!({ "type": "object", "properties": {} })),
                tool_descriptor("get_test_history", "Show recent runs and their per-result statuses for one test id.", json!({
                    "type": "object",
                    "properties": {
                        "test_id": { "type": "string", "description": "Test id, format suite::name" },
                        "days":    { "type": "integer", "default": 30 }
                    },
                    "required": ["test_id"]
                })),
                tool_descriptor("get_test_stats", "Show the Bayesian posterior + severity for one test id.", json!({
                    "type": "object",
                    "properties": { "test_id": { "type": "string" } },
                    "required": ["test_id"]
                })),
                tool_descriptor("get_quarantine_debt", "List currently quarantined tests with reason and author.", json!({ "type": "object", "properties": {} })),
                tool_descriptor("analyze_test", "Run the AI root-cause classifier for one test id. Requires ANTHROPIC_API_KEY.", json!({
                    "type": "object",
                    "properties": {
                        "test_id":  { "type": "string" },
                        "no_cache": { "type": "boolean", "default": false }
                    },
                    "required": ["test_id"]
                })),
            ]
        })),
        "tools/call" => handle_tool_call(req.id, req.params, store, ai_cfg, thr).await,
        "ping" => JsonRpcResponse::ok(req.id, json!({})),
        other => JsonRpcResponse::error(req.id, -32601, format!("unknown method: {other}")),
    }
}

fn tool_descriptor(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

async fn handle_tool_call(
    id: Option<Value>,
    params: Value,
    store: &Store,
    ai_cfg: &crate::domain::AiConfig,
    thr: &crate::domain::Thresholds,
) -> JsonRpcResponse {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let result = match name.as_str() {
        "list_flaky_tests"     => tool_list_flaky_tests(store, thr).await,
        "get_test_history"     => tool_get_test_history(store, &args).await,
        "get_test_stats"       => tool_get_test_stats(store, thr, &args).await,
        "get_quarantine_debt"  => tool_get_quarantine_debt(store).await,
        "analyze_test"         => tool_analyze_test(store, ai_cfg, &args).await,
        other                  => Err(FlaketideError::NotFound(format!("unknown tool: {other}"))),
    };
    match result {
        Ok(v) => JsonRpcResponse::ok(id, json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }],
            "structuredContent": v,
        })),
        Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
    }
}

async fn tool_list_flaky_tests(store: &Store, thr: &crate::domain::Thresholds) -> Result<Value> {
    let verdicts = stats::summarize(store, thr).await?;
    Ok(serde_json::to_value(&verdicts)?)
}

async fn tool_get_test_history(store: &Store, args: &Value) -> Result<Value> {
    let id = args.get("test_id").and_then(|v| v.as_str())
        .ok_or_else(|| FlaketideError::NotFound("test_id required".into()))?;
    let days = args.get("days").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
    let runs = store.history(days, Some(id)).await?;
    Ok(serde_json::to_value(&runs)?)
}

async fn tool_get_test_stats(store: &Store, thr: &crate::domain::Thresholds, args: &Value) -> Result<Value> {
    let id = args.get("test_id").and_then(|v| v.as_str())
        .ok_or_else(|| FlaketideError::NotFound("test_id required".into()))?;
    let verdicts = stats::summarize(store, thr).await?;
    let found = verdicts.into_iter().find(|v| v.id.as_str() == id);
    match found {
        Some(v) => Ok(serde_json::to_value(&v)?),
        None => Ok(json!({ "not_flaky": true, "test_id": id })),
    }
}

async fn tool_get_quarantine_debt(store: &Store) -> Result<Value> {
    let entries = store.list_quarantine().await?;
    Ok(serde_json::to_value(&entries)?)
}

async fn tool_analyze_test(
    store: &Store,
    ai_cfg: &crate::domain::AiConfig,
    args: &Value,
) -> Result<Value> {
    let id_str = args.get("test_id").and_then(|v| v.as_str())
        .ok_or_else(|| FlaketideError::NotFound("test_id required".into()))?;
    let no_cache = args.get("no_cache").and_then(|v| v.as_bool()).unwrap_or(false);
    let id = TestId::from_raw(id_str)?;
    let input = ClassifyInput::from_store(store, &id, ai_cfg.max_log_excerpt_chars).await?;
    let classifier = Classifier::from_env(ai_cfg.clone())?;
    let verdict = classifier.classify(store, &input, !no_cache).await?;
    Ok(serde_json::to_value(&verdict)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_ok_serialises() {
        let r = JsonRpcResponse::ok(Some(json!(7)), json!({"ok": true}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"id\":7"));
        assert!(s.contains("\"result\""));
        assert!(!s.contains("\"error\""));
    }

    #[test]
    fn response_error_serialises_without_result() {
        let r = JsonRpcResponse::error(Some(json!("abc")), -32601, "missing".into());
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains("\"id\":\"abc\""));
        assert!(s.contains("\"error\""));
        assert!(!s.contains("\"result\""));
    }
}
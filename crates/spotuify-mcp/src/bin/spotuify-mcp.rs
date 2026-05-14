//! `spotuify-mcp` -- JSON-RPC 2.0 over stdio bridge.
//!
//! Each line on stdin is a JSON-RPC request; each line on stdout is a
//! JSON-RPC response. Errors during framing are reported via stderr
//! and the process keeps running so editors can recover from bad
//! input without re-spawning.
//!
//! For `tools/call` of an executable tool, we forward the bridge's
//! translated `Request` over the Unix domain socket to a running
//! daemon. Daemon discovery uses `$SPOTUIFY_SOCKET` then falls back to
//! the OS cache dir.
//!
//! If the daemon is unreachable, we keep returning RPC results (so
//! the editor stays happy) but mark them with isError + a clear
//! message pointing the user to `spotuify daemon start`.

use std::io::{BufRead, Write};

use serde_json::{json, Value};
use spotuify_mcp::{
    bridge::{translate, TranslatedCall},
    confirm::{decide, Authorized},
    daemon_client::{default_socket_path, round_trip},
    dispatch, RpcRequest, RpcResponse,
};
use spotuify_protocol::Response;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    // One current-thread runtime for the whole session so each
    // tools/call doesn't pay tokio-runtime build cost.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(err) => {
                eprintln!("spotuify-mcp: stdin read error: {err}");
                break;
            }
        };

        let request: Result<RpcRequest, _> = serde_json::from_str(&line);
        let response = match request {
            Ok(req) => handle(req, &rt),
            Err(err) => RpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(spotuify_mcp::RpcError::invalid_request(format!(
                    "parse: {err}"
                ))),
            },
        };

        match serde_json::to_string(&response) {
            Ok(line) => {
                if writeln!(out, "{line}").is_err() {
                    break;
                }
            }
            Err(err) => {
                eprintln!("spotuify-mcp: response serialize error: {err}");
            }
        }
        let _ = out.flush();
    }
}

/// Intercept `tools/call` so we can forward an executable Request to
/// the live daemon. Other methods (initialize, tools/list, resources/*)
/// stay catalogue-only and go through the pure-function dispatch.
fn handle(request: RpcRequest, rt: &tokio::runtime::Runtime) -> RpcResponse {
    if request.method == "tools/call" {
        let id = request.id.clone().unwrap_or(Value::Null);
        let params = request.params.clone();
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        let confirm = args.get("confirm").and_then(Value::as_bool);

        // Reuse the existing dispatch logic for catalogue lookup +
        // confirm gating. For Execute paths, we follow up with a
        // daemon round-trip.
        match decide(&name, confirm) {
            Err(err) => {
                return RpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(spotuify_mcp::RpcError::invalid_request(err.to_string())),
                };
            }
            Ok(Authorized::PreviewOnly) => {
                // Fall through to the pure dispatch for the preview
                // path; nothing to forward.
                return dispatch(request);
            }
            Ok(Authorized::Execute) => match translate(&name, &args) {
                Ok(TranslatedCall::Request(req)) => {
                    let socket = default_socket_path();
                    let outcome = rt.block_on(round_trip(&socket, req));
                    return daemon_outcome_to_rpc(id, outcome);
                }
                Ok(TranslatedCall::LocalDeferred(_)) => {
                    return dispatch(request);
                }
                Err(err) => {
                    return RpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: None,
                        error: Some(spotuify_mcp::RpcError::invalid_params(err.to_string())),
                    };
                }
            },
        }
    }

    dispatch(request)
}

fn daemon_outcome_to_rpc(id: Value, outcome: anyhow::Result<Response>) -> RpcResponse {
    match outcome {
        Ok(Response::Ok { data }) => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Daemon ok: {:?}", data),
                }],
                "_meta": {
                    "spotuify_response_kind": kind_label(&data),
                }
            })),
            error: None,
        },
        Ok(Response::Error { message, code, .. }) => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Daemon error [{code}]: {message}"),
                }],
                "isError": true,
            })),
            error: None,
        },
        Err(err) => RpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(json!({
                "content": [{
                    "type": "text",
                    "text": format!(
                        "Daemon unreachable: {err}. Start it with `spotuify daemon start`."
                    ),
                }],
                "isError": true,
            })),
            error: None,
        },
    }
}

fn kind_label(data: &spotuify_protocol::ResponseData) -> &'static str {
    use spotuify_protocol::ResponseData as D;
    match data {
        D::Pong => "pong",
        D::Shutdown => "shutdown",
        D::DaemonStatus { .. } => "daemon-status",
        D::DoctorReport { .. } => "doctor",
        D::Playback { .. } => "playback",
        D::Devices { .. } => "devices",
        D::SearchResults { .. } => "search-results",
        D::CacheStatus { .. } => "cache-status",
        D::Reindex { .. } => "reindex",
        D::Sync { .. } => "sync",
        D::Image { .. } => "image",
        D::Queue { .. } => "queue",
        D::Playlists { .. } => "playlists",
        D::MediaItems { .. } => "media-items",
        D::Logs { .. } => "logs",
        D::Mutation { .. } => "mutation",
        D::PlaylistCreate { .. } => "playlist-create",
        // Phase 10 — analytics responses
        D::AnalyticsTop { .. } => "analytics-top",
        D::AnalyticsHabits { .. } => "analytics-habits",
        D::AnalyticsSearch { .. } => "analytics-search",
        D::AnalyticsRediscovery { .. } => "analytics-rediscovery",
        D::AnalyticsRebuildReport { .. } => "analytics-rebuild",
        D::AnalyticsPruneReport { .. } => "analytics-prune",
        // Phase 12 — operations responses
        D::Operations { .. } => "operations",
        D::OperationDetail { .. } => "operation-detail",
        D::OperationUndoResult { .. } => "operation-undo-result",
    }
}

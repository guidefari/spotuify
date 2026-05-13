//! `spotuify-mcp` -- JSON-RPC 2.0 over stdio bridge.
//!
//! Each line on stdin is a JSON-RPC request; each line on stdout is a
//! JSON-RPC response. Errors during framing are reported via stderr
//! and the process keeps running so editors can recover from bad
//! input without re-spawning.
//!
//! The daemon-side dispatch (forwarding translated Requests over the
//! Unix socket to the live daemon) lands as a follow-up. For now this
//! binary serves the catalogue + bridge translation; clients can
//! integrate with the manifest and confirm flow before the daemon
//! wire is hot.

use std::io::{BufRead, Write};

use spotuify_mcp::{dispatch, RpcRequest, RpcResponse};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if l.trim().is_empty() => continue,
            Ok(l) => l,
            Err(err) => {
                eprintln!("spotuify-mcp: stdin read error: {err}");
                break;
            }
        };

        let response = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(req) => dispatch(req),
            Err(err) => RpcResponse {
                jsonrpc: "2.0",
                id: serde_json::Value::Null,
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

//! JSON-RPC 2.0 over stdio transport for `spotuify-mcp`.

use std::io::{BufRead, Write};

use serde_json::Value;

use crate::{server::handle_request, RpcError, RpcRequest, RpcResponse};

pub fn run() -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => line,
            Err(err) => {
                eprintln!("spotuify-mcp: stdin read error: {err}");
                break;
            }
        };

        let request: Result<RpcRequest, _> = serde_json::from_str(&line);
        let response = match request {
            Ok(req) => rt.block_on(handle_request(req)),
            Err(err) => RpcResponse {
                jsonrpc: "2.0",
                id: Value::Null,
                result: None,
                error: Some(RpcError::invalid_request(format!("parse: {err}"))),
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

    Ok(())
}

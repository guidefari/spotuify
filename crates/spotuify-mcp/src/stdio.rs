//! JSON-RPC 2.0 over stdio transport for `spotuify-mcp`.
//!
//! All outbound lines — request responses and server-initiated
//! `notifications/resources/updated` — funnel through one mpsc channel
//! to a single writer thread, so the request loop and the daemon-event
//! pusher never interleave partial lines on stdout.

use std::io::{BufRead, Write};
use std::sync::mpsc;

use serde_json::{json, Value};

use crate::{server::handle_request, RpcError, RpcRequest, RpcResponse};

pub fn run() -> anyhow::Result<()> {
    let (out_tx, out_rx) = mpsc::channel::<String>();

    // Single writer: the only thing that touches stdout.
    let writer = std::thread::spawn(move || {
        let stdout = std::io::stdout();
        let mut out = stdout.lock();
        for line in out_rx {
            if writeln!(out, "{line}").is_err() || out.flush().is_err() {
                break;
            }
        }
    });

    // Background: stream daemon events and push resource-updated
    // notifications for subscribed URIs. Best-effort — reconnects if the
    // daemon isn't up yet, and stays silent when nothing is subscribed.
    spawn_resource_push(out_tx.clone());

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => line,
            Err(err) => {
                eprintln!("spotuify-mcp: stdin read error: {err}");
                break;
            }
        };

        let response = match serde_json::from_str::<RpcRequest>(&line) {
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
                if out_tx.send(line).is_err() {
                    break;
                }
            }
            Err(err) => eprintln!("spotuify-mcp: response serialize error: {err}"),
        }
    }

    // stdin closed → drop our sender so the writer drains and exits.
    drop(out_tx);
    let _ = writer.join();
    Ok(())
}

/// Spawn the daemon-event → `notifications/resources/updated` pusher on
/// its own runtime/thread. The detached thread lives for the process;
/// `run()` returning on stdin EOF exits the process and reclaims it.
fn spawn_resource_push(out_tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(async move {
            loop {
                if let Ok(mut client) = spotuify_protocol::ipc_client::IpcClient::connect().await {
                    while let Ok(event) = client.next_event().await {
                        if !push_event(&out_tx, &event) {
                            return; // writer gone → client disconnected
                        }
                    }
                }
                // Daemon down or stream dropped — back off and retry.
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        });
    });
}

/// Map an event to the subscribed resource URIs it invalidates and push
/// one notification per URI. Returns false when the writer channel is
/// closed (the client disconnected).
fn push_event(out_tx: &mpsc::Sender<String>, event: &spotuify_protocol::DaemonEvent) -> bool {
    let Some(tag) = crate::resources::event_invalidation_tag(event) else {
        return true;
    };
    let subscribed = crate::rpc::subscribed_uris();
    for uri in crate::resources::resource_uris_invalidated_by(tag) {
        if !subscribed.contains(uri) {
            continue;
        }
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/updated",
            "params": { "uri": uri },
        });
        if out_tx.send(notification.to_string()).is_err() {
            return false;
        }
    }
    true
}

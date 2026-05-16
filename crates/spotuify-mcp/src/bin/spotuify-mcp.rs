//! `spotuify-mcp` -- JSON-RPC 2.0 over stdio bridge.

fn main() -> anyhow::Result<()> {
    spotuify_mcp::stdio::run()
}

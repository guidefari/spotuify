//! Manifest golden test. Any addition/removal/rename of an MCP tool
//! trips this test so the protocol surface change is visible in PR diffs.

use spotuify_mcp::ToolManifest;

#[test]
fn mcp_manifest_matches_snapshot() {
    let manifest = ToolManifest::build();
    insta::assert_yaml_snapshot!(manifest);
}

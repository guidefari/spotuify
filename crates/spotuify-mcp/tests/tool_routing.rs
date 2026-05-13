//! MCP tool catalogue + bridge + confirmation tests.

use serde_json::json;
use spotuify_mcp::bridge::{translate, BridgeError, TranslatedCall};
use spotuify_mcp::confirm::{decide, Authorized, ConfirmDecision};
use spotuify_mcp::tools::{ToolCatalogue, ToolKind};

// --- Catalogue invariants ---

#[test]
fn catalogue_has_no_duplicate_tool_names() {
    let mut names: Vec<&str> = ToolCatalogue::all().iter().map(|t| t.name).collect();
    names.sort();
    let mut dedup = names.clone();
    dedup.dedup();
    assert_eq!(names, dedup, "duplicate tool name detected");
}

#[test]
fn destructive_tools_iterator_matches_destructive_flag() {
    let from_flag: Vec<&str> = ToolCatalogue::all()
        .iter()
        .filter(|t| t.destructive)
        .map(|t| t.name)
        .collect();
    let from_iter: Vec<&str> = ToolCatalogue::destructive().map(|t| t.name).collect();
    assert_eq!(from_flag, from_iter);
}

#[test]
fn every_destructive_tool_has_kind_destructive() {
    for t in ToolCatalogue::destructive() {
        assert_eq!(
            t.kind,
            ToolKind::Destructive,
            "tool {} is destructive=true but kind!=Destructive",
            t.name
        );
    }
}

#[test]
fn by_name_lookup_matches_iteration() {
    for t in ToolCatalogue::all() {
        assert_eq!(ToolCatalogue::by_name(t.name), Some(t));
    }
    assert!(ToolCatalogue::by_name("not_a_tool").is_none());
}

// --- Confirmation gating ---

#[test]
fn read_tools_authorize_to_execute_without_confirm() {
    let result = decide("search", None).unwrap();
    assert_eq!(result, Authorized::Execute);
}

#[test]
fn transport_tools_authorize_to_execute_without_confirm() {
    let result = decide("play", None).unwrap();
    assert_eq!(result, Authorized::Execute);
}

#[test]
fn destructive_tool_without_confirm_yields_preview_only() {
    let result = decide("playlist_create", None).unwrap();
    assert_eq!(result, Authorized::PreviewOnly);

    let result = decide("playlist_create", Some(false)).unwrap();
    assert_eq!(result, Authorized::PreviewOnly);
}

#[test]
fn destructive_tool_with_confirm_true_authorizes_execute() {
    let result = decide("playlist_create", Some(true)).unwrap();
    assert_eq!(result, Authorized::Execute);
}

#[test]
fn unknown_tool_returns_unknown_tool_error() {
    match decide("not_a_real_tool", None) {
        Err(ConfirmDecision::UnknownTool(name)) => assert_eq!(name, "not_a_real_tool"),
        other => panic!("expected UnknownTool, got {other:?}"),
    }
}

#[test]
fn undo_last_is_not_destructive_so_no_confirm_required() {
    let result = decide("undo_last", None).unwrap();
    assert_eq!(
        result,
        Authorized::Execute,
        "undo_last should execute without confirm -- it is the safety net"
    );
}

// --- Bridge translation ---

#[test]
fn search_tool_translates_to_request_search() {
    let call = translate("search", &json!({"query": "luther vandross"})).unwrap();
    match call {
        TranslatedCall::Request(spotuify_protocol::Request::Search { query, limit, .. }) => {
            assert_eq!(query, "luther vandross");
            assert_eq!(limit, 20, "default limit should be 20");
        }
        other => panic!("expected Search request, got {other:?}"),
    }
}

#[test]
fn search_tool_clamps_excessive_limit_to_50() {
    let call = translate("search", &json!({"query": "x", "limit": 1000})).unwrap();
    match call {
        TranslatedCall::Request(spotuify_protocol::Request::Search { limit, .. }) => {
            assert_eq!(limit, 50);
        }
        other => panic!("expected Search request, got {other:?}"),
    }
}

#[test]
fn now_playing_translates_to_playback_get() {
    let call = translate("now_playing", &json!({})).unwrap();
    assert!(matches!(
        call,
        TranslatedCall::Request(spotuify_protocol::Request::PlaybackGet)
    ));
}

#[test]
fn play_uri_requires_uri_arg() {
    let err = translate("play_uri", &json!({})).unwrap_err();
    match err {
        BridgeError::MissingArg { tool, arg } => {
            assert_eq!(tool, "play_uri");
            assert_eq!(arg, "uri");
        }
        other => panic!("expected MissingArg, got {other:?}"),
    }
}

#[test]
fn play_uri_with_wrong_type_yields_bad_arg_type() {
    let err = translate("play_uri", &json!({"uri": 42})).unwrap_err();
    match err {
        BridgeError::BadArgType { tool, arg } => {
            assert_eq!(tool, "play_uri");
            assert_eq!(arg, "uri");
        }
        other => panic!("expected BadArgType, got {other:?}"),
    }
}

#[test]
fn playlist_create_translates_with_uris_array() {
    let call = translate(
        "playlist_create",
        &json!({
            "name": "Focus",
            "description": "deep work",
            "uris": ["spotify:track:1", "spotify:track:2"]
        }),
    )
    .unwrap();
    match call {
        TranslatedCall::Request(spotuify_protocol::Request::PlaylistCreate {
            name,
            description,
            uris,
        }) => {
            assert_eq!(name, "Focus");
            assert_eq!(description.as_deref(), Some("deep work"));
            assert_eq!(uris.len(), 2);
        }
        other => panic!("expected PlaylistCreate, got {other:?}"),
    }
}

#[test]
fn playlist_create_with_missing_name_errors() {
    let err = translate("playlist_create", &json!({"description": "x"})).unwrap_err();
    match err {
        BridgeError::MissingArg { arg, .. } => assert_eq!(arg, "name"),
        other => panic!("expected MissingArg name, got {other:?}"),
    }
}

#[test]
fn pause_translates_to_playback_command_pause() {
    let call = translate("pause", &json!({})).unwrap();
    match call {
        TranslatedCall::Request(spotuify_protocol::Request::PlaybackCommand { command }) => {
            assert!(matches!(command, spotuify_protocol::PlaybackCommand::Pause));
        }
        other => panic!("expected PlaybackCommand::Pause, got {other:?}"),
    }
}

#[test]
fn deferred_tools_signal_via_local_deferred() {
    let call = translate("lyrics", &json!({})).unwrap();
    assert!(matches!(call, TranslatedCall::LocalDeferred(_)));

    let call = translate("ops_log", &json!({})).unwrap();
    assert!(matches!(call, TranslatedCall::LocalDeferred(_)));

    let call = translate("undo_last", &json!({})).unwrap();
    assert!(matches!(call, TranslatedCall::LocalDeferred(_)));
}

#[test]
fn unknown_tool_returns_bridge_unknown_tool() {
    let err = translate("not_a_tool", &json!({})).unwrap_err();
    match err {
        BridgeError::UnknownTool(name) => assert_eq!(name, "not_a_tool"),
        other => panic!("expected UnknownTool, got {other:?}"),
    }
}

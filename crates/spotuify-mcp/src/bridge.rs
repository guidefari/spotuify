//! MCP tool → spotuify-protocol Request bridge.
//!
//! Translates JSON-shaped MCP tool calls into the typed Request enum the
//! daemon already understands. Pure functions, trivially testable. The
//! actual MCP transport (rmcp stdio/HTTP) is a thin wrapper around
//! these.

use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("tool `{tool}` requires argument `{arg}`")]
    MissingArg { tool: String, arg: String },
    #[error("tool `{tool}` arg `{arg}` had wrong type")]
    BadArgType { tool: String, arg: String },
    #[error("tool `{0}` not implemented yet (gated on later phases)")]
    NotYetImplemented(String),
    #[error("tool `{0}` not in catalogue")]
    UnknownTool(String),
}

/// Pull a required string arg out of a tool-call args object.
pub fn required_str<'a>(args: &'a Value, tool: &str, key: &str) -> Result<&'a str, BridgeError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| match args.get(key) {
            None => BridgeError::MissingArg {
                tool: tool.into(),
                arg: key.into(),
            },
            Some(_) => BridgeError::BadArgType {
                tool: tool.into(),
                arg: key.into(),
            },
        })
}

pub fn optional_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

pub fn optional_u64(args: &Value, key: &str) -> Option<u64> {
    args.get(key).and_then(Value::as_u64)
}

pub fn optional_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(Value::as_bool)
}

/// Result of a successful tool-call translation. The bridge layer wraps
/// this in MCP JSON-RPC framing; the daemon consumes the inner Request.
#[derive(Debug)]
pub enum TranslatedCall {
    /// Forward to the daemon as a typed Request.
    Request(spotuify_protocol::Request),
    /// Run a local read-only query (analytics, ops log) implemented as a
    /// daemon Request once Phases 10/12 land. For now: NotImplemented.
    LocalDeferred(&'static str),
}

/// Translate `(tool_name, args)` into either a daemon Request or a
/// deferred-feature marker.
pub fn translate(
    tool: &str,
    args: &Value,
) -> Result<TranslatedCall, BridgeError> {
    use spotuify_protocol::PlaybackCommand;
    use spotuify_protocol::Request as R;

    match tool {
        "search" => {
            let query = required_str(args, tool, "query")?.to_string();
            let scope = parse_scope(optional_str(args, "kind"));
            let source = parse_source(optional_str(args, "source"));
            let limit = optional_u64(args, "limit").map(|n| n.min(50) as u32).unwrap_or(20);
            Ok(TranslatedCall::Request(R::Search {
                query,
                scope,
                source,
                limit,
            }))
        }
        "now_playing" => Ok(TranslatedCall::Request(R::PlaybackGet)),
        "devices_list" => Ok(TranslatedCall::Request(R::DevicesList)),
        "queue_show" => Ok(TranslatedCall::Request(R::QueueGet)),
        "playlists_list" => Ok(TranslatedCall::Request(R::PlaylistsList)),
        "playlist_tracks" => {
            let playlist = required_str(args, tool, "playlist")?.to_string();
            Ok(TranslatedCall::Request(R::PlaylistTracks { playlist }))
        }
        "library_list" => {
            let limit = optional_u64(args, "limit").map(|n| n.min(500) as u32).unwrap_or(100);
            Ok(TranslatedCall::Request(R::LibraryList { limit }))
        }
        "play" | "play_uri" => {
            // The MCP "play" tool requires a URI -- LLMs are expected to
            // call `search` first when they have a name. That keeps the
            // flow predictable and avoids LLM hallucination of URIs that
            // get treated as "best match".
            let uri = required_str(args, tool, "uri")?.to_string();
            Ok(TranslatedCall::Request(R::PlaybackCommand {
                command: PlaybackCommand::PlayUri { uri },
            }))
        }
        "pause" => Ok(TranslatedCall::Request(R::PlaybackCommand {
            command: PlaybackCommand::Pause,
        })),
        "resume" => Ok(TranslatedCall::Request(R::PlaybackCommand {
            command: PlaybackCommand::Resume,
        })),
        "next" => Ok(TranslatedCall::Request(R::PlaybackCommand {
            command: PlaybackCommand::Next,
        })),
        "previous" => Ok(TranslatedCall::Request(R::PlaybackCommand {
            command: PlaybackCommand::Previous,
        })),
        "seek" => {
            let position_ms = optional_u64(args, "position_ms").ok_or_else(|| {
                BridgeError::MissingArg {
                    tool: tool.into(),
                    arg: "position_ms".into(),
                }
            })?;
            Ok(TranslatedCall::Request(R::PlaybackCommand {
                command: PlaybackCommand::Seek { position_ms },
            }))
        }
        "volume" => {
            let volume_percent = optional_u64(args, "percent")
                .ok_or_else(|| BridgeError::MissingArg {
                    tool: tool.into(),
                    arg: "percent".into(),
                })?
                .min(100) as u8;
            Ok(TranslatedCall::Request(R::PlaybackCommand {
                command: PlaybackCommand::Volume { volume_percent },
            }))
        }
        "shuffle" => {
            let state = optional_bool(args, "on").ok_or_else(|| BridgeError::MissingArg {
                tool: tool.into(),
                arg: "on".into(),
            })?;
            Ok(TranslatedCall::Request(R::PlaybackCommand {
                command: PlaybackCommand::Shuffle { state },
            }))
        }
        "repeat" => {
            let state = required_str(args, tool, "mode")?.to_string();
            Ok(TranslatedCall::Request(R::PlaybackCommand {
                command: PlaybackCommand::Repeat { state },
            }))
        }
        "queue_add" => {
            let uri = required_str(args, tool, "uri")?.to_string();
            Ok(TranslatedCall::Request(R::QueueAdd { uri }))
        }
        "transfer_device" => {
            let device = required_str(args, tool, "device")?.to_string();
            Ok(TranslatedCall::Request(R::DeviceTransfer { device }))
        }
        "playlist_create" => {
            let name = required_str(args, tool, "name")?.to_string();
            let description = optional_str(args, "description").map(str::to_string);
            let uris = args
                .get("uris")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Ok(TranslatedCall::Request(R::PlaylistCreate {
                name,
                description,
                uris,
            }))
        }
        "playlist_add" => {
            let playlist = required_str(args, tool, "playlist")?.to_string();
            let uris = args
                .get("uris")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .ok_or_else(|| BridgeError::MissingArg {
                    tool: tool.into(),
                    arg: "uris".into(),
                })?;
            Ok(TranslatedCall::Request(R::PlaylistAddItems { playlist, uris }))
        }
        "library_save" | "library_unsave" => {
            let uri = required_str(args, tool, "uri")?.to_string();
            // The legacy LibrarySave request carries Option<String> because
            // it also supports "current track" mode. MCP always supplies
            // an explicit URI.
            Ok(TranslatedCall::Request(R::LibrarySave {
                uri: Some(uri),
                current: false,
            }))
        }
        // Deferred until Phase 9/10/12 land.
        "lyrics"
        | "radio_start"
        | "related_artists"
        | "analytics_top"
        | "analytics_habits"
        | "ops_log"
        | "undo_last"
        | "playlist_remove" => Ok(TranslatedCall::LocalDeferred(static_label(tool))),
        other => Err(BridgeError::UnknownTool(other.to_string())),
    }
}

fn parse_scope(raw: Option<&str>) -> spotuify_protocol::SearchScopeData {
    use spotuify_protocol::SearchScopeData as S;
    match raw.unwrap_or("track") {
        "track" => S::Track,
        "episode" => S::Episode,
        "album" => S::Album,
        "artist" => S::Artist,
        "playlist" => S::Playlist,
        _ => S::All,
    }
}

fn parse_source(raw: Option<&str>) -> spotuify_protocol::SearchSourceData {
    use spotuify_protocol::SearchSourceData as S;
    match raw.unwrap_or("hybrid") {
        "local" => S::Local,
        "spotify" => S::Spotify,
        _ => S::Hybrid,
    }
}

fn static_label(tool: &str) -> &'static str {
    match tool {
        "lyrics" => "lyrics (Phase 9 / embedded backend required)",
        "radio_start" => "radio_start (Phase 9 / embedded backend required)",
        "related_artists" => "related_artists (Phase 9 / embedded backend required)",
        "analytics_top" => "analytics_top (Phase 10)",
        "analytics_habits" => "analytics_habits (Phase 10)",
        "ops_log" => "ops_log (Phase 12)",
        "undo_last" => "undo_last (Phase 12)",
        "playlist_remove" => "playlist_remove (queued for daemon support)",
        _ => "deferred",
    }
}

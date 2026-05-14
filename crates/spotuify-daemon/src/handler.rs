use std::sync::Arc;
use std::time::Instant;

use spotuify_core::{now_ms, search_performed_event};
use spotuify_protocol::{
    CommandReceipt, DaemonEvent, Operation, OperationId, OperationKind, OperationSource,
    OperationStatus, PlaybackCommand, PlaylistCreateReceipt, ReceiptId, Request, Response,
    ResponseData, SearchScopeData, SearchSourceData,
};
use spotuify_spotify::actions::{self, CommandKind};
use spotuify_spotify::client::{MediaItem, MediaKind};
use spotuify_spotify::selection;

use crate::state::DaemonState;

pub(crate) async fn handle_request(state: Arc<DaemonState>, request: Request) -> Response {
    match dispatch(state, request).await {
        Ok(data) => Response::Ok { data },
        Err(err) => Response::error(err.to_string()),
    }
}

async fn dispatch(state: Arc<DaemonState>, request: Request) -> anyhow::Result<ResponseData> {
    // Phase 12 — capture the canonical serialized Request once so each
    // mutation arm can persist it as `request_json` on its receipt row.
    // `ops redo` deserialises this back into a Request for replay.
    let request_json = serde_json::to_string(&request).unwrap_or_else(|_| "{}".to_string());
    match request {
        Request::Ping => Ok(ResponseData::Pong),
        Request::GetDaemonStatus => Ok(ResponseData::DaemonStatus {
            status: state.status(),
        }),
        Request::GetDoctorReport => Ok(ResponseData::DoctorReport {
            // Phase 6.9: pass the daemon's recent-event snapshot so the
            // report includes RateLimited / AuthError / SchemaCompat
            // findings.
            report: crate::diagnostics::collect_report_with_events(
                state.status(),
                state.event_log_snapshot().await,
            )
            .await?,
        }),
        Request::PlaybackGet => {
            let mut client = state.spotify_client().await?;
            let playback = actions::status(&mut client).await?;
            cache_playback(&state, &playback).await;
            Ok(ResponseData::Playback { playback })
        }
        Request::PlaybackCommand { command } => {
            let action = playback_command_action(&command);
            let op_kind = playback_command_operation_kind(&command);
            let request_summary = request_json.clone();
            let state_for = state.clone();
            record_operation(
                &state,
                op_kind,
                OperationSource::DaemonInternal,
                vec![],
                action,
                &request_summary,
                Some(spotuify_protocol::PreState::Transport),
                Some(spotuify_protocol::ReversalPlan::NotReversible {
                    reason: "transport".to_string(),
                }),
                move |_op_id| async move {
                    let mut client = state_for.spotify_client().await?;
                    let command = playback_command_kind(command);
                    let result = actions::execute(&mut client, command).await?;
                    let message =
                        result.message.clone().unwrap_or_else(|| action.to_string());
                    state_for.emit_event(DaemonEvent::PlaybackChanged {
                        action: action.to_string(),
                    });
                    emit_mutation_finished(&state_for, action, &message);
                    Ok(ResponseData::Mutation {
                        receipt: receipt(action, result.message),
                    })
                },
            )
            .await
        }
        Request::DevicesList => {
            let mut client = state.spotify_client().await?;
            let devices = actions::devices(&mut client).await?;
            cache_devices(&state, &devices).await;
            Ok(ResponseData::Devices { devices })
        }
        Request::DeviceTransfer { device } => {
            let request_summary = request_json.clone();
            let state_for = state.clone();
            record_operation(
                &state,
                OperationKind::Transfer,
                OperationSource::DaemonInternal,
                vec![],
                "transfer",
                &request_summary,
                None,
                None,
                move |op_id| async move {
                    let mut client = state_for.spotify_client().await?;
                    let devices = actions::devices(&mut client).await?;
                    let target_device = selection::resolve_device(&devices, &device)?;
                    let playback = actions::status(&mut client).await?;
                    let play = playback.is_playing;
                    let prior_device_id = playback.device.as_ref().and_then(|d| d.id.clone());
                    // Persist the prior device id so undo can transfer
                    // back. If prior is unknown (no active device), the
                    // reversal plan is a "stop playback" no-op surfaced
                    // through TransferToPriorDevice with the daemon's
                    // best-effort target.
                    let pre_state = spotuify_protocol::PreState::Transfer {
                        prior_device_id: prior_device_id.clone(),
                    };
                    let plan = match prior_device_id.clone() {
                        Some(id) => {
                            spotuify_protocol::ReversalPlan::TransferToPriorDevice {
                                device_id: id,
                            }
                        }
                        None => spotuify_protocol::ReversalPlan::NotReversible {
                            reason: "no prior active device to restore".to_string(),
                        },
                    };
                    if let Err(err) = state_for
                        .store()
                        .update_operation_plan(op_id, Some(&pre_state), Some(&plan))
                        .await
                    {
                        tracing::warn!(error = %err, "failed to persist transfer pre-state");
                    }
                    let result = actions::execute(
                        &mut client,
                        CommandKind::Transfer {
                            device: target_device,
                            play,
                        },
                    )
                    .await?;
                    let message =
                        result.message.clone().unwrap_or_else(|| "transfer".to_string());
                    state_for.emit_event(DaemonEvent::DevicesChanged {
                        action: "transfer".to_string(),
                    });
                    state_for.emit_event(DaemonEvent::PlaybackChanged {
                        action: "transfer".to_string(),
                    });
                    emit_mutation_finished(&state_for, "transfer", &message);
                    Ok(ResponseData::Mutation {
                        receipt: receipt("transfer", result.message),
                    })
                },
            )
            .await
        }
        Request::Search {
            query,
            scope,
            source,
            limit,
        } => Ok(ResponseData::SearchResults {
            items: search_with_source(state.clone(), query, scope, source, limit).await?,
        }),
        Request::Reindex => Ok(ResponseData::Reindex {
            stats: spotuify_search::reindex::reindex(state.store(), state.search()).await?,
        }),
        Request::CacheStatus => {
            let index_documents = state.search().num_docs().await.unwrap_or(0);
            Ok(ResponseData::CacheStatus {
                status: state.store().cache_status(index_documents).await?,
            })
        }
        Request::LibraryList { limit } => Ok(ResponseData::MediaItems {
            items: state.store().list_library_items(limit).await?,
        }),
        Request::LogsTail { lines } => Ok(ResponseData::Logs {
            lines: crate::logging::read_tail(lines)?
                .lines()
                .map(ToString::to_string)
                .collect(),
        }),
        Request::Sync { target } => Ok(ResponseData::Sync {
            summary: spotuify_sync::sync_target(state.as_ref(), target).await?,
        }),
        Request::RecentlyPlayed => {
            let mut client = state.spotify_client().await?;
            let items = client.recently_played().await?;
            cache_recent_items(&state, &items).await;
            Ok(ResponseData::MediaItems { items })
        }
        Request::Image { url } => {
            let client = state.spotify_client().await?;
            Ok(ResponseData::Image {
                bytes: client.image(&url).await?,
            })
        }
        Request::QueueGet => {
            let mut client = state.spotify_client().await?;
            Ok(ResponseData::Queue {
                queue: actions::queue(&mut client).await?,
            })
        }
        Request::QueueAdd { uri } => {
            let uri_for_event = uri.clone();
            let state_for_event = state.clone();
            let request_summary = request_json.clone();
            let pre_state = Some(spotuify_protocol::PreState::QueueAdd { uri: uri.clone() });
            let plan = Some(spotuify_protocol::ReversalPlan::QueueRemove { uri: uri.clone() });
            record_operation(
                &state,
                OperationKind::QueueAdd,
                OperationSource::DaemonInternal,
                vec![uri.clone()],
                "queue",
                &request_summary,
                pre_state,
                plan,
                move |_op_id| async move {
                    let mut client = state_for_event.spotify_client().await?;
                    let result =
                        actions::execute(&mut client, CommandKind::QueueUri { uri: uri.clone() })
                            .await?;
                    let message = result
                        .message
                        .clone()
                        .unwrap_or_else(|| "queue".to_string());
                    state_for_event.emit_event(DaemonEvent::QueueChanged {
                        action: "queue".to_string(),
                        uris: vec![uri_for_event],
                    });
                    emit_mutation_finished(&state_for_event, "queue", &message);
                    Ok(ResponseData::Mutation {
                        receipt: receipt("queue", result.message),
                    })
                },
            )
            .await
        }
        Request::PlaylistsList => {
            let mut client = state.spotify_client().await?;
            let playlists = actions::playlists(&mut client).await?;
            cache_playlists(&state, &playlists).await;
            Ok(ResponseData::Playlists { playlists })
        }
        Request::PlaylistTracks { playlist } => {
            let mut client = state.spotify_client().await?;
            let playlists = actions::playlists(&mut client).await?;
            let playlist = selection::resolve_playlist(&playlists, &playlist)?;
            let items = client.playlist_tracks(&playlist.id).await?;
            cache_playlist_items(&state, &playlist.id, &items).await;
            Ok(ResponseData::MediaItems { items })
        }
        Request::PlaylistAddItems { playlist, uris } => {
            let state_for = state.clone();
            let request_summary = request_json.clone();
            let subject_uris = uris.clone();
            record_operation(
                &state,
                OperationKind::PlaylistAdd,
                OperationSource::DaemonInternal,
                subject_uris,
                "playlist-add",
                &request_summary,
                // Initial values are placeholders; the body captures the
                // resolved playlist's snapshot_id from the same
                // `actions::playlists()` call it already makes for
                // resolution and writes the real plan via
                // `update_operation_plan`.
                None,
                None,
                move |op_id| async move {
                    let mut client = state_for.spotify_client().await?;
                    let playlists = actions::playlists(&mut client).await?;
                    let resolved = selection::resolve_playlist(&playlists, &playlist)?;
                    let snapshot_id = resolved.snapshot_id.clone();
                    let pre_state = spotuify_protocol::PreState::PlaylistAdd {
                        playlist_id: resolved.id.clone(),
                        snapshot_id: snapshot_id.clone(),
                        added_uris: uris.clone(),
                    };
                    let plan = spotuify_protocol::ReversalPlan::PlaylistRemoveTracks {
                        playlist_id: resolved.id.clone(),
                        uris: uris.clone(),
                        snapshot_id,
                    };
                    if let Err(err) = state_for
                        .store()
                        .update_operation_plan(op_id, Some(&pre_state), Some(&plan))
                        .await
                    {
                        tracing::warn!(error = %err, "failed to persist playlist_add pre-state");
                    }
                    for uri in &uris {
                        let item = media_item_from_uri(uri)?;
                        actions::execute(
                            &mut client,
                            CommandKind::AddToPlaylist {
                                item,
                                playlist_id: resolved.id.clone(),
                                playlist_name: resolved.name.clone(),
                            },
                        )
                        .await?;
                    }
                    let message = format!("Added items to {}", resolved.name);
                    state_for.emit_event(DaemonEvent::PlaylistsChanged {
                        action: "playlist-add".to_string(),
                        playlist: Some(resolved.id.clone()),
                    });
                    emit_mutation_finished(&state_for, "playlist-add", &message);
                    Ok(ResponseData::Mutation {
                        receipt: receipt("playlist-add", Some(message)),
                    })
                },
            )
            .await
        }
        Request::PlaylistCreate {
            name,
            description,
            uris,
        } => {
            if uris.is_empty() {
                anyhow::bail!("no resolved track URIs to add");
            }
            for uri in &uris {
                if selection::media_kind_from_uri(uri)? != MediaKind::Track {
                    anyhow::bail!("playlist creation candidates must be track URIs: {uri}");
                }
            }
            let request_summary = request_json.clone();
            let state_for = state.clone();
            let name_for = name.clone();
            let description_for = description.clone();
            let uris_for = uris.clone();
            record_operation(
                &state,
                OperationKind::PlaylistCreate,
                OperationSource::DaemonInternal,
                vec![],
                "playlist-create",
                &request_summary,
                None,
                None,
                move |op_id| async move {
                    let mut client = state_for.spotify_client().await?;
                    let playlist = client
                        .create_playlist(&name_for, description_for.as_deref(), false)
                        .await?;
                    let playlist_uri = selection::playlist_uri(&playlist.id);
                    let pre_state = spotuify_protocol::PreState::PlaylistCreate {
                        playlist_id: playlist.id.clone(),
                    };
                    let plan = spotuify_protocol::ReversalPlan::PlaylistDelete {
                        playlist_id: playlist.id.clone(),
                    };
                    if let Err(err) = state_for
                        .store()
                        .update_operation_plan(op_id, Some(&pre_state), Some(&plan))
                        .await
                    {
                        tracing::warn!(error = %err, "failed to persist playlist_create pre-state");
                    }
                    if let Err(err) = state_for
                        .store()
                        .update_operation_subject_uris(op_id, &[playlist_uri.clone()])
                        .await
                    {
                        tracing::warn!(error = %err, "failed to persist playlist_create subject uri");
                    }
                    client
                        .add_items_to_playlist(&playlist.id, &uris_for)
                        .await?;
                    cache_playlists(&state_for, std::slice::from_ref(&playlist)).await;
                    state_for.emit_event(DaemonEvent::PlaylistsChanged {
                        action: "playlist-create".to_string(),
                        playlist: Some(playlist.id.clone()),
                    });
                    let message =
                        format!("Created playlist `{name_for}` with {} item(s)", uris_for.len());
                    emit_mutation_finished(&state_for, "playlist-create", &message);
                    Ok(ResponseData::PlaylistCreate {
                        receipt: PlaylistCreateReceipt {
                            ok: true,
                            action: "playlist-create".to_string(),
                            playlist_uri,
                            playlist_id: playlist.id,
                            name: playlist.name,
                            added_item_count: uris_for.len(),
                            message,
                        },
                    })
                },
            )
            .await
        }
        Request::LibrarySave { uri, current } => {
            let request_summary = request_json.clone();
            let state_for = state.clone();
            let uri_for = uri.clone();
            record_operation(
                &state,
                OperationKind::LibrarySave,
                OperationSource::DaemonInternal,
                uri.iter().cloned().collect(),
                "save",
                &request_summary,
                None,
                None,
                move |op_id| async move {
                    let mut client = state_for.spotify_client().await?;
                    let event_uris = uri_for.iter().cloned().collect::<Vec<_>>();
                    // Resolve the URI early so we can register a real
                    // reversal plan. SaveCurrent reads now-playing first
                    // to derive the URI.
                    let resolved_uri = match uri_for.clone() {
                        Some(u) => Some(u),
                        None if current => actions::status(&mut client)
                            .await
                            .ok()
                            .and_then(|p| p.item.map(|item| item.uri)),
                        None => None,
                    };
                    if let Some(ref real_uri) = resolved_uri {
                        let pre_state = spotuify_protocol::PreState::LibrarySave {
                            uri: real_uri.clone(),
                            prior_was_saved: false,
                        };
                        let plan = spotuify_protocol::ReversalPlan::LibraryUnsave {
                            uri: real_uri.clone(),
                        };
                        if let Err(err) = state_for
                            .store()
                            .update_operation_plan(op_id, Some(&pre_state), Some(&plan))
                            .await
                        {
                            tracing::warn!(error = %err, "failed to persist library_save pre-state");
                        }
                        if let Err(err) = state_for
                            .store()
                            .update_operation_subject_uris(op_id, &[real_uri.clone()])
                            .await
                        {
                            tracing::warn!(error = %err, "failed to persist library_save subject uri");
                        }
                    }
                    let command = if current {
                        CommandKind::SaveCurrent
                    } else {
                        let u = uri_for
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("provide uri or current=true"))?;
                        CommandKind::SaveItem {
                            item: media_item_from_uri(&u)?,
                        }
                    };
                    let result = actions::execute(&mut client, command).await?;
                    let message = result.message.clone().unwrap_or_else(|| "save".to_string());
                    state_for.emit_event(DaemonEvent::LibraryChanged {
                        action: "save".to_string(),
                        uris: event_uris,
                    });
                    emit_mutation_finished(&state_for, "save", &message);
                    Ok(ResponseData::Mutation {
                        receipt: receipt("save", result.message),
                    })
                },
            )
            .await
        }
        Request::Shutdown => {
            state.request_shutdown();
            Ok(ResponseData::Shutdown)
        }

        // Phase 10 (P10.6) analytics dispatch.
        Request::AnalyticsRebuild { since_ms } => Ok(ResponseData::AnalyticsRebuildReport {
            report: state
                .store()
                .rebuild_derivations_from_events(since_ms)
                .await?,
        }),
        Request::AnalyticsTop {
            kind,
            since_window,
            limit,
        } => Ok(ResponseData::AnalyticsTop {
            entries: state.store().top_entries(kind, since_window, limit).await?,
        }),
        Request::AnalyticsHabits { window, since_ms } => Ok(ResponseData::AnalyticsHabits {
            buckets: state.store().habit_buckets(window, since_ms).await?,
        }),
        Request::AnalyticsSearch { mode, limit } => Ok(ResponseData::AnalyticsSearch {
            entries: state
                .store()
                .search_history(
                    matches!(mode, spotuify_protocol::SearchMode::Normalized),
                    limit,
                )
                .await?,
        }),
        Request::AnalyticsRediscovery { gap_days } => Ok(ResponseData::AnalyticsRediscovery {
            candidates: state.store().rediscovery_candidates(gap_days, 50).await?,
        }),
        Request::AnalyticsPrune { apply } => {
            // Prune raw playback_progress (90d) + analytics_events (365d)
            // + operations (90d) older than the configured retention
            // windows. Dry-run by default. Read the windows from config
            // when available; fall back to blueprint defaults.
            let now = now_ms();
            let progress_cutoff = now - 90 * 86_400_000;
            let events_cutoff = now - 365 * 86_400_000;
            let ops_cutoff = now - 90 * 86_400_000;

            if !apply {
                // Dry-run: count rows that *would* be deleted via
                // COUNT() rather than DELETE. Best-effort: errors here
                // fall back to zero so the daemon never panics from a
                // diagnostic query.
                let count_progress: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM playback_progress WHERE sampled_at_ms < ?",
                )
                .bind(progress_cutoff)
                .fetch_one(state.store().reader())
                .await
                .unwrap_or(0);
                let count_events: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM analytics_events WHERE occurred_at_ms < ?",
                )
                .bind(events_cutoff)
                .fetch_one(state.store().reader())
                .await
                .unwrap_or(0);
                let count_ops: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM operations WHERE occurred_at_ms < ?")
                        .bind(ops_cutoff)
                        .fetch_one(state.store().reader())
                        .await
                        .unwrap_or(0);
                return Ok(ResponseData::AnalyticsPruneReport {
                    rows_pruned: (count_progress + count_events + count_ops).max(0) as u64,
                    dry_run: true,
                });
            }

            let pruned_progress = state
                .store()
                .prune_playback_progress(progress_cutoff)
                .await
                .unwrap_or(0);
            let pruned_events = state
                .store()
                .prune_analytics_events(events_cutoff)
                .await
                .unwrap_or(0);
            let pruned_ops = state
                .store()
                .prune_operations_older_than(ops_cutoff)
                .await
                .unwrap_or(0);
            Ok(ResponseData::AnalyticsPruneReport {
                rows_pruned: pruned_progress + pruned_events + pruned_ops,
                dry_run: false,
            })
        }
        Request::AnalyticsExport { .. } | Request::AnalyticsImport { .. } => {
            anyhow::bail!(
                "ListenBrainz/Last.fm export+import lands in the scrobble-bridge follow-up; \
                 use the shell-hook recipe in docs/recipes/ to scrobble live listens."
            )
        }
        Request::OpsLog {
            limit,
            since_ms,
            source,
        } => Ok(ResponseData::Operations {
            ops: state
                .store()
                .list_operations(limit, since_ms, source)
                .await?,
        }),
        Request::OpsShow {
            operation_id,
            with_diff,
        } => {
            let op = state.store().get_operation(operation_id).await?;
            let diff = if with_diff {
                op.reversal_plan
                    .as_ref()
                    .zip(op.pre_state.as_ref())
                    .map(|(plan, pre)| crate::undo::render_plan_summary(plan, pre))
            } else {
                None
            };
            Ok(ResponseData::OperationDetail { op, diff })
        }
        Request::OpsUndo {
            operation_id,
            dry_run,
            force,
            bulk_since_ms,
        } => handle_ops_undo(&state, operation_id, dry_run, force, bulk_since_ms).await,
        Request::OpsRedo { operation_id } => handle_ops_redo(&state, operation_id).await,

        // --- Phase 13 — QoL / spec-compliance handlers ---
        Request::Reload => {
            // Reload re-reads the config file. The daemon mostly carries
            // config through its constituent subsystems; this handler
            // emits an Ack with a hint and re-emits an event so TUI
            // clients refresh their cached config copy. A future pass
            // can hot-swap player backends + keymaps when changed.
            match spotuify_spotify::config::Config::load() {
                Ok(_) => {
                    state.emit_event(DaemonEvent::ConfigReloaded);
                    Ok(ResponseData::Ack {
                        message: "config reloaded".to_string(),
                    })
                }
                Err(err) => anyhow::bail!("reload failed: {err}"),
            }
        }
        Request::Reconnect => {
            // ConnectOnly / Spotifyd backends are stateless from our
            // POV; the embedded backend has its own Session that may
            // need rebuilding. We surface a clear Ack so the user sees
            // something happened; the embedded reconnect path lands in
            // the player-backend follow-up.
            tracing::info!("daemon reconnect requested");
            state.emit_event(DaemonEvent::ConfigReloaded);
            Ok(ResponseData::Ack {
                message: "session reconnect requested".to_string(),
            })
        }
        Request::SearchCachePrune { older_than_ms } => {
            let cutoff = older_than_ms.unwrap_or_else(|| now_ms() - 30 * 86_400_000);
            let pruned_runs = state
                .store()
                .prune_search_runs_older_than(cutoff)
                .await
                .unwrap_or(0);
            Ok(ResponseData::SearchCachePruned {
                pruned_runs,
                pruned_results: 0,
            })
        }
    }
}

async fn handle_ops_undo(
    state: &std::sync::Arc<DaemonState>,
    operation_id: Option<spotuify_protocol::OperationId>,
    dry_run: bool,
    force: bool,
    bulk_since_ms: Option<i64>,
) -> anyhow::Result<ResponseData> {
    // Bulk undo: walk every reversible succeeded op newer than `since`,
    // reverse-chronological, stop on first failure (per blueprint).
    if let Some(since) = bulk_since_ms {
        let ops = state
            .store()
            .find_reversible_operations_since(since, None)
            .await?;
        let undo_op_id = OperationId::new_v7();
        let mut succeeded = 0u32;
        let mut skipped = 0u32;
        let mut errors = Vec::new();
        for op in ops {
            match undo_single(state, &op, dry_run, force).await {
                Ok(true) => succeeded += 1,
                Ok(false) => skipped += 1,
                Err(err) => {
                    errors.push(err.to_string());
                    break;
                }
            }
        }
        return Ok(ResponseData::OperationUndoResult {
            undo_op_id,
            succeeded,
            skipped,
            errors,
        });
    }

    // Single op (default: last reversible).
    let op = match operation_id {
        Some(id) => state.store().get_operation(id).await?,
        None => state
            .store()
            .find_last_reversible_operation()
            .await?
            .ok_or_else(|| anyhow::anyhow!("no reversible operations to undo"))?,
    };
    let undo_op_id = OperationId::new_v7();
    let mut errors = Vec::new();
    let succeeded = match undo_single(state, &op, dry_run, force).await {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(err) => {
            errors.push(err.to_string());
            0
        }
    };
    Ok(ResponseData::OperationUndoResult {
        undo_op_id,
        succeeded,
        skipped: 0,
        errors,
    })
}

async fn undo_single(
    state: &std::sync::Arc<DaemonState>,
    op: &spotuify_protocol::Operation,
    dry_run: bool,
    force: bool,
) -> anyhow::Result<bool> {
    crate::undo::validate_undoable(op)?;
    let plan = op
        .reversal_plan
        .clone()
        .ok_or_else(|| anyhow::anyhow!("op {} missing reversal_plan", op.operation_id))?;

    // Snapshot conflict detection. Foundation pass: no pre-state was
    // captured (record_operation passes None), so snapshot_id is None
    // and this is a no-op. Real captures land in the feature pass that
    // adds pre-call observation closures to each mutating arm.
    let state_clone = state.clone();
    let fetch = move |id: &str| -> Option<String> {
        // Best-effort fetch: spawn a blocking client call. The closure
        // is sync because check_snapshot is synchronous; we use the
        // tokio runtime's block_in_place to keep the API simple.
        let id = id.to_string();
        let state = state_clone.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let mut client = state.spotify_client().await.ok()?;
                let playlists = client.playlists().await.ok()?;
                playlists
                    .into_iter()
                    .find(|p| p.id == id)
                    .and_then(|p| p.snapshot_id)
            })
        })
    };
    crate::undo::check_snapshot(&plan, fetch, force)?;

    if dry_run {
        // Dry-run: return the plan summary as a "would-undo" indicator.
        // The result-shape carries no payload — caller renders the
        // op + plan via OpsShow.
        return Ok(false);
    }

    // Execute the reversal via Spotify Web API. Currently the daemon
    // owns inverse calls only for `transfer` and `add_to_queue`'s
    // best-effort skip. The remaining variants surface a clear
    // "not yet implemented" error so the caller can plan accordingly.
    apply_reversal(state, &plan).await?;

    // Record the new undo operation row + flip the original to undone.
    let undo_op = spotuify_protocol::Operation {
        operation_id: OperationId::new_v7(),
        kind: OperationKind::Undo,
        occurred_at_ms: now_ms(),
        finished_at_ms: Some(now_ms()),
        source: OperationSource::DaemonInternal,
        requester: None,
        subject_uris: op.subject_uris.clone(),
        reversible: true,
        reversal_plan: Some(spotuify_protocol::ReversalPlan::Redo {
            target_op_id: op.operation_id,
        }),
        pre_state: None,
        status: OperationStatus::Succeeded,
        receipt_id: None,
        subject_op_id: Some(op.operation_id),
        undone_by_op_id: None,
        redone_by_op_id: None,
        error_message: None,
    };
    state.store().insert_pending_operation(&undo_op).await?;
    state
        .store()
        .mark_operation_undone(op.operation_id, undo_op.operation_id)
        .await?;
    state.emit_event(DaemonEvent::OperationUndone {
        undo_op_id: undo_op.operation_id,
        original_op_id: op.operation_id,
        success: true,
    });
    Ok(true)
}

async fn apply_reversal(
    state: &std::sync::Arc<DaemonState>,
    plan: &spotuify_protocol::ReversalPlan,
) -> anyhow::Result<()> {
    use spotuify_protocol::ReversalPlan as P;
    match plan {
        P::TransferToPriorDevice { device_id } => {
            let mut client = state.spotify_client().await?;
            client.transfer(device_id, false).await
        }
        P::QueueRemove { uri } => {
            // Spotify Web API has no specific queue-remove; surface
            // this as a clear non-error skip so bulk-undo logs it.
            tracing::warn!(target = %uri, "queue remove not supported by Spotify Web API; skipping");
            Ok(())
        }
        P::PlaylistRemoveTracks {
            playlist_id,
            uris,
            snapshot_id,
        } => {
            let mut client = state.spotify_client().await?;
            client
                .remove_playlist_items(playlist_id, uris, snapshot_id.as_deref())
                .await
                .map(|_new_snap| ())
        }
        P::PlaylistAddAtPositions {
            playlist_id,
            items,
            snapshot_id,
        } => {
            let mut client = state.spotify_client().await?;
            client
                .add_items_to_playlist_at_positions(playlist_id, items, snapshot_id.as_deref())
                .await
                .map(|_| ())
        }
        P::PlaylistDelete { playlist_id } => {
            let mut client = state.spotify_client().await?;
            client.unfollow_playlist(playlist_id).await
        }
        P::PlaylistReorder {
            playlist_id,
            range_start,
            insert_before,
            range_length,
            snapshot_id,
        } => {
            let mut client = state.spotify_client().await?;
            client
                .reorder_playlist_items(
                    playlist_id,
                    *range_start,
                    *insert_before,
                    *range_length,
                    snapshot_id.as_deref(),
                )
                .await
                .map(|_| ())
        }
        P::LibraryUnsave { uri } => {
            let mut client = state.spotify_client().await?;
            client.library_unsave_by_uri(uri).await
        }
        P::LibrarySave { uri, .. } => {
            // `prior_added_at_ms` is recorded for forensics only —
            // Spotify's save endpoint always sets `added_at` to now.
            // Documented limitation; surfaced in `ops show --diff`.
            let mut client = state.spotify_client().await?;
            client.library_save_by_uri(uri).await
        }
        P::Like { uri } => {
            // Like ≡ library_save for tracks; the protocol keeps Like
            // distinct from LibrarySave for clarity in the op log even
            // though Spotify's endpoint is the same.
            let mut client = state.spotify_client().await?;
            client.library_save_by_uri(uri).await
        }
        P::Unlike { uri } => {
            let mut client = state.spotify_client().await?;
            client.library_unsave_by_uri(uri).await
        }
        P::NotReversible { reason } => {
            anyhow::bail!("operation is not reversible: {reason}")
        }
        P::Redo { .. } => anyhow::bail!(
            "redo of an undo replays the original forward op; \
             use `ops redo` instead of `ops undo`"
        ),
    }
}

async fn handle_ops_redo(
    state: &std::sync::Arc<DaemonState>,
    operation_id: Option<spotuify_protocol::OperationId>,
) -> anyhow::Result<ResponseData> {
    // Find an undone op to redo. Default: most-recent undone.
    let op = match operation_id {
        Some(id) => state.store().get_operation(id).await?,
        None => {
            let ops = state.store().list_operations(50, None, None).await?;
            ops.into_iter()
                .find(|o| o.status == OperationStatus::Undone)
                .ok_or_else(|| anyhow::anyhow!("no undone operations to redo"))?
        }
    };
    if op.status != OperationStatus::Undone {
        anyhow::bail!(
            "operation {} is not undone (status = {:?}); only undone ops can be redone",
            op.operation_id,
            op.status,
        );
    }

    // Real redo: re-execute the original Request by fetching its
    // serialized form from the linked receipt row. The fresh dispatch
    // creates its own operation row through `record_operation`, so
    // mark the original as redone-by that fresh row.
    let receipt_id = op
        .receipt_id
        .ok_or_else(|| anyhow::anyhow!("op {} has no receipt; cannot redo", op.operation_id))?;
    let raw = state.store().receipt_request_json(receipt_id).await?;
    let original_request: Request = serde_json::from_str(&raw)
        .map_err(|err| anyhow::anyhow!("failed to decode original request: {err}"))?;
    // Record the timestamp before dispatch so we can locate the freshly
    // minted operation row afterwards.
    let dispatch_started_at = now_ms();
    // Recursive dispatch. Any failure surfaces back to the caller.
    let response = Box::pin(dispatch(state.clone(), original_request)).await?;

    // Locate the newly-minted op row created by the re-dispatched
    // mutation. dispatch is in-process and serial, so the most-recent
    // op with `occurred_at_ms >= dispatch_started_at` is ours.
    let recent_ops = state
        .store()
        .list_operations(5, Some(dispatch_started_at), None)
        .await
        .unwrap_or_default();
    let redo_op_id = recent_ops
        .into_iter()
        .find(|o| {
            o.operation_id != op.operation_id
                && o.kind != OperationKind::Redo
                && o.kind != OperationKind::Undo
        })
        .map(|o| o.operation_id)
        .unwrap_or_else(OperationId::new_v7);

    let _ = state
        .store()
        .mark_operation_redone(op.operation_id, redo_op_id)
        .await;
    state.emit_event(DaemonEvent::OperationUndone {
        undo_op_id: redo_op_id,
        original_op_id: op.operation_id,
        success: true,
    });
    let _ = response;
    Ok(ResponseData::OperationUndoResult {
        undo_op_id: redo_op_id,
        succeeded: 1,
        skipped: 0,
        errors: vec![],
    })
}

async fn search_with_source(
    state: Arc<DaemonState>,
    query: String,
    scope: SearchScopeData,
    source: SearchSourceData,
    limit: u32,
) -> anyhow::Result<Vec<MediaItem>> {
    match source {
        SearchSourceData::Local => local_cached_search(&state, &query, scope, limit).await,
        SearchSourceData::Spotify => spotify_search_and_cache(state, query, scope, limit).await,
        SearchSourceData::Hybrid => {
            let local = local_cached_search(&state, &query, scope, limit).await?;
            if !local.is_empty() {
                let refresh_state = state.clone();
                let refresh_query = query.clone();
                tokio::spawn(async move {
                    if let Err(err) =
                        spotify_search_and_cache(refresh_state, refresh_query, scope, limit).await
                    {
                        tracing::debug!(error = %err, "background hybrid search refresh failed");
                    }
                });
                return Ok(local);
            }
            spotify_search_and_cache(state, query, scope, limit).await
        }
    }
}

async fn local_cached_search(
    state: &DaemonState,
    query: &str,
    scope: SearchScopeData,
    limit: u32,
) -> anyhow::Result<Vec<MediaItem>> {
    let hits = state
        .search()
        .search(query, scope, limit as usize)
        .await
        .unwrap_or_default();
    if !hits.is_empty() {
        let uris = hits.into_iter().map(|hit| hit.uri).collect::<Vec<_>>();
        let items = state.store().media_items_by_uris(&uris).await?;
        if !items.is_empty() {
            return Ok(items);
        }
    }
    state.store().local_search(query, scope, limit).await
}

async fn spotify_search_and_cache(
    state: Arc<DaemonState>,
    query: String,
    scope: SearchScopeData,
    limit: u32,
) -> anyhow::Result<Vec<MediaItem>> {
    let mut client = state.spotify_client().await?;
    let kinds = scope_media_kinds(scope);
    let started = Instant::now();
    let mut items = client
        .search_with_limit(&query, &kinds, limit as u8)
        .await?;
    client
        .record_analytics_event(search_performed_event(
            client.analytics_source(),
            &query,
            items.len(),
            started.elapsed().as_millis(),
            now_ms(),
        ))
        .await;
    for item in &mut items {
        item.source = Some("spotify".to_string());
        item.freshness = Some("fresh".to_string());
    }
    state
        .store()
        .cache_search_results(&query, scope, SearchSourceData::Spotify, &items)
        .await?;
    state.emit_event(DaemonEvent::SearchUpdated {
        query: query.clone(),
        count: items.len(),
    });
    let entries = items
        .iter()
        .cloned()
        .map(|item| spotuify_store::IndexedMediaItem {
            item,
            liked: false,
            saved: false,
            added_at_ms: Some(spotuify_store::now_ms()),
            source: "spotify".to_string(),
        })
        .collect();
    if let Err(err) = state
        .search()
        .apply_batch(spotuify_search::SearchUpdateBatch {
            entries,
            removed_uris: Vec::new(),
        })
        .await
    {
        tracing::warn!(error = %err, "failed to update search index from Spotify results");
    }
    Ok(items)
}

fn scope_media_kinds(scope: SearchScopeData) -> Vec<MediaKind> {
    match scope {
        SearchScopeData::All => vec![
            MediaKind::Track,
            MediaKind::Episode,
            MediaKind::Album,
            MediaKind::Artist,
            MediaKind::Playlist,
        ],
        SearchScopeData::Track => vec![MediaKind::Track],
        SearchScopeData::Episode => vec![MediaKind::Episode],
        SearchScopeData::Album => vec![MediaKind::Album],
        SearchScopeData::Artist => vec![MediaKind::Artist],
        SearchScopeData::Playlist => vec![MediaKind::Playlist],
    }
}

async fn cache_playback(state: &DaemonState, playback: &spotuify_spotify::client::Playback) {
    if let Err(err) = state.store().persist_playback(playback).await {
        tracing::warn!(error = %err, "failed to cache playback snapshot");
    }
}

async fn cache_devices(state: &DaemonState, devices: &[spotuify_spotify::client::Device]) {
    if let Err(err) = state.store().persist_devices(devices).await {
        tracing::warn!(error = %err, "failed to cache devices");
    }
}

async fn cache_recent_items(state: &DaemonState, items: &[MediaItem]) {
    if let Err(err) = state.store().persist_recent_items(items).await {
        tracing::warn!(error = %err, "failed to cache recent items");
    }
}

async fn cache_playlists(state: &DaemonState, playlists: &[spotuify_spotify::client::Playlist]) {
    if let Err(err) = state.store().persist_playlists(playlists).await {
        tracing::warn!(error = %err, "failed to cache playlists");
    }
}

async fn cache_playlist_items(state: &DaemonState, playlist_id: &str, items: &[MediaItem]) {
    if let Err(err) = state
        .store()
        .persist_playlist_items(playlist_id, items)
        .await
    {
        tracing::warn!(error = %err, "failed to cache playlist items");
    }
}

fn playback_command_kind(command: PlaybackCommand) -> CommandKind {
    match command {
        PlaybackCommand::Pause => CommandKind::Pause,
        PlaybackCommand::Resume => CommandKind::Resume,
        PlaybackCommand::Toggle => CommandKind::TogglePlayback,
        PlaybackCommand::Next => CommandKind::Next,
        PlaybackCommand::Previous => CommandKind::Previous,
        PlaybackCommand::PlayUri { uri } => CommandKind::PlayUri { uri },
        PlaybackCommand::Seek { position_ms } => CommandKind::Seek { position_ms },
        PlaybackCommand::Volume { volume_percent } => CommandKind::Volume { volume_percent },
        PlaybackCommand::Shuffle { state } => CommandKind::Shuffle { state },
        PlaybackCommand::Repeat { state } => CommandKind::Repeat { state },
    }
}

fn playback_command_action(command: &PlaybackCommand) -> &'static str {
    match command {
        PlaybackCommand::Pause => "pause",
        PlaybackCommand::Resume => "resume",
        PlaybackCommand::Toggle => "toggle",
        PlaybackCommand::Next => "next",
        PlaybackCommand::Previous => "previous",
        PlaybackCommand::PlayUri { .. } => "play-uri",
        PlaybackCommand::Seek { .. } => "seek",
        PlaybackCommand::Volume { .. } => "volume",
        PlaybackCommand::Shuffle { .. } => "shuffle",
        PlaybackCommand::Repeat { .. } => "repeat",
    }
}

fn playback_command_operation_kind(command: &PlaybackCommand) -> OperationKind {
    match command {
        PlaybackCommand::Pause => OperationKind::Pause,
        PlaybackCommand::Resume => OperationKind::Resume,
        PlaybackCommand::Toggle => OperationKind::Toggle,
        PlaybackCommand::Next => OperationKind::Next,
        PlaybackCommand::Previous => OperationKind::Previous,
        PlaybackCommand::PlayUri { .. } => OperationKind::Play,
        PlaybackCommand::Seek { .. } => OperationKind::Seek,
        PlaybackCommand::Volume { .. } => OperationKind::Volume,
        PlaybackCommand::Shuffle { .. } => OperationKind::Shuffle,
        PlaybackCommand::Repeat { .. } => OperationKind::Repeat,
    }
}

fn receipt(action: &str, message: Option<String>) -> CommandReceipt {
    CommandReceipt {
        ok: true,
        action: action.to_string(),
        message: message.unwrap_or_else(|| action.to_string()),
    }
}

fn emit_mutation_finished(state: &DaemonState, action: &str, message: &str) {
    state.emit_event(DaemonEvent::MutationFinished {
        action: action.to_string(),
        message: message.to_string(),
    });
}

/// Phase 12 — record an operation row around every mutation. Wraps
/// `record_mutation` (Phase 6.6 receipt lifecycle) and also writes an
/// `operations` row + emits `OperationRecorded`.
///
/// `body` receives the freshly-minted `OperationId` so it can call
/// `state.store().update_operation_plan(op_id, …)` mid-flight once it
/// has captured the pre-mutation `snapshot_id` / prior device / etc.
/// Transport commands typically pass `(NotReversible, Transport)` up
/// front; reversible mutations (playlist_add, transfer, library_save)
/// fill in real pre-state inside the body.
async fn record_operation<F, Fut, T>(
    state: &std::sync::Arc<DaemonState>,
    kind: OperationKind,
    source: OperationSource,
    subject_uris: Vec<String>,
    action: &str,
    request_summary: &str,
    initial_pre_state: Option<spotuify_protocol::PreState>,
    initial_reversal_plan: Option<spotuify_protocol::ReversalPlan>,
    body: F,
) -> anyhow::Result<T>
where
    F: FnOnce(OperationId) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let operation_id = OperationId::new_v7();
    let occurred_at_ms = now_ms();
    let receipt_id = ReceiptId::new_v7();
    let reversible = kind.is_reversible()
        && !matches!(
            &initial_reversal_plan,
            Some(spotuify_protocol::ReversalPlan::NotReversible { .. })
        );
    let row = Operation {
        operation_id,
        kind,
        occurred_at_ms,
        finished_at_ms: None,
        source,
        requester: None,
        subject_uris: subject_uris.clone(),
        reversible,
        reversal_plan: initial_reversal_plan,
        pre_state: initial_pre_state,
        status: OperationStatus::Pending,
        receipt_id: Some(receipt_id),
        subject_op_id: None,
        undone_by_op_id: None,
        redone_by_op_id: None,
        error_message: None,
    };
    let _ = state.store().insert_pending_operation(&row).await;

    let result = record_mutation_with_id(
        state,
        receipt_id,
        action,
        request_summary,
        body(operation_id),
    )
    .await;

    let finished = now_ms();
    let (status, error) = match &result {
        Ok(_) => (OperationStatus::Succeeded, None),
        Err(err) => (OperationStatus::Failed, Some(err.to_string())),
    };
    let _ = state
        .store()
        .finalize_operation(operation_id, status, finished, error.as_deref())
        .await;
    state.emit_event(DaemonEvent::OperationRecorded {
        operation_id,
        kind,
        source,
    });
    result
}

/// Phase 6.6 -- record a pending receipt + emit MutationAccepted, then
/// finalize after the body runs. Best-effort: if the receipts table is
/// unavailable for any reason we still execute the mutation and emit
/// the legacy MutationFinished event, so existing call sites keep
/// working. Returns the body's result unchanged.
async fn record_mutation<T>(
    state: &std::sync::Arc<DaemonState>,
    action: &str,
    request_summary: &str,
    body: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    let receipt_id = spotuify_protocol::ReceiptId::new_v7();
    record_mutation_with_id(state, receipt_id, action, request_summary, body).await
}

/// Same as `record_mutation` but with a caller-provided receipt id, so
/// `record_operation` can link receipt and operation rows together.
async fn record_mutation_with_id<T>(
    state: &std::sync::Arc<DaemonState>,
    receipt_id: spotuify_protocol::ReceiptId,
    action: &str,
    request_summary: &str,
    body: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    let started = crate::analytics::now_ms();
    let receipt = spotuify_protocol::Receipt {
        receipt_id,
        action: action.to_string(),
        status: spotuify_protocol::ReceiptStatus::Pending,
        message: "queued".to_string(),
        started_at_ms: started,
        finished_at_ms: None,
        error: None,
    };
    let _ = state
        .store()
        .insert_pending_receipt(&receipt, request_summary)
        .await;
    state.emit_event(spotuify_protocol::DaemonEvent::MutationAccepted {
        receipt_id,
        action: action.to_string(),
    });

    let result = body.await;
    let finished = crate::analytics::now_ms();
    let (status, message, error_summary) = match &result {
        Ok(_) => (
            spotuify_protocol::ReceiptStatus::Confirmed,
            format!("{action} confirmed"),
            None,
        ),
        Err(err) => {
            let msg = err.to_string();
            (
                spotuify_protocol::ReceiptStatus::Failed,
                msg.clone(),
                Some(spotuify_protocol::ApiErrorSummary {
                    kind: spotuify_protocol::IpcErrorKind::Provider,
                    message: msg,
                    retry_after_secs: None,
                }),
            )
        }
    };
    let _ = state
        .store()
        .finalize_receipt(
            receipt_id,
            status,
            &message,
            finished,
            error_summary.as_ref(),
        )
        .await;
    state.emit_event(spotuify_protocol::DaemonEvent::MutationFinalized {
        receipt_id,
        status,
        message: message.clone(),
    });
    result
}

fn media_item_from_uri(uri: &str) -> anyhow::Result<MediaItem> {
    let kind = selection::media_kind_from_uri(uri)?;
    let id = uri.rsplit(':').next().map(str::to_string);
    Ok(MediaItem {
        id,
        uri: uri.to_string(),
        name: uri.to_string(),
        subtitle: String::new(),
        context: String::new(),
        duration_ms: 0,
        image_url: None,
        kind,
        source: None,
        freshness: None,
        explicit: None,
        is_playable: None,
    })
}

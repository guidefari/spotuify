//! `media` request handlers (split out of the dispatch god-function).

#![allow(unused_imports)]
use std::sync::Arc;
use std::time::{Duration, Instant};

use spotuify_core::{now_ms, search_performed_event, Playback};
use spotuify_protocol::{
    CommandReceipt, DaemonEvent, EpisodeSort, Operation, OperationId, OperationKind,
    OperationSource, OperationStatus, PlaybackCommand, PlaylistCreateReceipt, ReceiptId, Request,
    Response, ResponseData, SearchScopeData, SearchSortData, SearchSourceData,
};
use spotuify_spotify::actions::{self, CommandKind};
use spotuify_spotify::client::{MediaItem, MediaKind, SpotifyClient};
use spotuify_spotify::config::Config;
use spotuify_spotify::selection;

use crate::analytics::AnalyticsStore;
use crate::handler::*;
use crate::retention::retention_cutoffs;
use crate::state::{DaemonState, FastTransportStatus};

pub(crate) async fn dispatch(
    state: Arc<DaemonState>,
    request: Request,
    _source: Option<OperationSource>,
) -> anyhow::Result<ResponseData> {
    match request {
        Request::Image { url } => {
            let entry = state
                .system_integration
                .cover_cache
                .get_or_fetch_entry(&url)
                .await?;
            Ok(ResponseData::Image {
                bytes: tokio::fs::read(entry.path).await?,
            })
        }
        Request::CoverArt { url } => {
            let entry = state
                .system_integration
                .cover_cache
                .get_or_fetch_entry(&url)
                .await?;
            Ok(ResponseData::CoverArt {
                path: entry.path.display().to_string(),
                cache_hit: entry.cache_hit,
                bytes: entry.bytes,
                fetched_at_ms: entry.fetched_at_ms,
            })
        }
        Request::LyricsGet {
            track_uri,
            force_refresh,
        } => lyrics_get(state, track_uri, force_refresh).await,
        Request::LyricsOffsetSet {
            track_uri,
            offset_ms,
        } => {
            state
                .store()
                .set_lyrics_offset_ms(&track_uri, offset_ms)
                .await?;
            Ok(ResponseData::LyricsOffset {
                track_uri,
                offset_ms,
            })
        }
        _ => unreachable!("non-media request routed to media dispatcher"),
    }
}

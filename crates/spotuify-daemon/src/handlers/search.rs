//! `search` request handlers (split out of the dispatch god-function).

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
        Request::Search {
            query,
            scope,
            source,
            limit,
            kinds,
            sort,
        } => Ok(ResponseData::SearchResults {
            items: search_with_source(state.clone(), query, scope, source, limit, kinds, sort)
                .await?,
        }),
        Request::SearchStream {
            query,
            scope,
            source,
            version,
        } => {
            spawn_search_stream(state.clone(), query.clone(), scope, source, version);
            Ok(ResponseData::SearchStarted { query, version })
        }
        Request::SearchPage {
            query,
            kind,
            offset,
            version,
        } => {
            spawn_search_page(state.clone(), query.clone(), kind, offset, version);
            Ok(ResponseData::SearchStarted { query, version })
        }
        _ => unreachable!("non-search request routed to search dispatcher"),
    }
}
